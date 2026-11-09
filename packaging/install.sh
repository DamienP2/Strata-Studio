#!/usr/bin/env bash
# Strata Studio · installateur pas à pas pour Ubuntu 22.04 et 24.04 (amd64).
#
#   curl -fsSLO https://github.com/DamienP2/Strata-Studio/releases/latest/download/install.sh
#   sudo bash install.sh
#
# Options :
#   --yes            sans question : Studio installé, les appareils à régler
#                    ensuite dans l'administration (http://<serveur>:8095/admin)
#   --answers FICHIER  les réponses dans un fichier (CLE=valeur), sans question
#   --deb FICHIER    installer ce paquet plutôt que celui de la dernière release
#   --version X.Y.Z  installer cette release
#
# L'installateur peut être relancé : il reprend les réglages en place et
# ne change que ce qu'on lui dit.
set -euo pipefail

REPO="DamienP2/Strata-Studio"
DATA=/var/lib/strata-studio
ETC=/etc/strata-studio
SETTINGS="$DATA/reglages.json"
LOG=/var/log/strata-studio-install.log
TITLE="Strata Studio · installation"
YES=0
ANSWERS=""
DEB=""
VERSION=""
STEP=0
STEPS=14

while [ $# -gt 0 ]; do
    case "$1" in
        --yes|-y) YES=1 ;;
        --answers) ANSWERS="$2"; shift ;;
        --deb) DEB="$2"; shift ;;
        --version) VERSION="${2#v}"; shift ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) echo "Option inconnue : $1" >&2; exit 2 ;;
    esac
    shift
done

if [ "$(id -u)" -ne 0 ]; then
    echo "Lancez l'installateur en administrateur : sudo bash $0" >&2
    exit 1
fi
touch "$LOG"
exec 3>&1
log() { echo "[$(date '+%F %T')] $*" >> "$LOG"; }
say() { echo "  $*" >&3; log "$*"; }
fail() { echo "Erreur : $*" >&2; log "ERREUR $*"; exit 1; }
run() { log "$ $*"; "$@" >> "$LOG" 2>&1; }

# ---------- the questions ----------

declare -A A=()
if [ -n "$ANSWERS" ]; then
    [ -f "$ANSWERS" ] || fail "fichier de réponses introuvable : $ANSWERS"
    while IFS='=' read -r k v; do
        [[ -z "$k" || "$k" =~ ^# ]] && continue
        A["$k"]="$v"
    done < "$ANSWERS"
fi
quiet() { [ "$YES" = 1 ] || [ -n "$ANSWERS" ]; }
has_dialog() { ! quiet && command -v whiptail >/dev/null 2>&1 && [ -t 0 ]; }

step() {
    STEP=$((STEP + 1))
    echo >&3
    echo "── Étape $STEP sur $STEPS · $1" >&3
    log "== $1"
}

# ask KEY "question" "défaut" : a line of text.
ask() {
    local key="$1" q="$2" def="${3:-}" out
    if [ -n "${A[$key]+x}" ]; then echo "${A[$key]}"; return; fi
    if quiet; then echo "$def"; return; fi
    if has_dialog; then
        out=$(whiptail --title "$TITLE" --backtitle "Étape $STEP sur $STEPS" --inputbox "$q" 12 74 "$def" 3>&1 1>&2 2>&3) || out="$def"
    else
        read -r -p "  $q [$def] " out </dev/tty || true
        out="${out:-$def}"
    fi
    echo "$out"
}

# secret KEY "question" : a password (not shown).
secret() {
    local key="$1" q="$2" out=""
    if [ -n "${A[$key]+x}" ]; then echo "${A[$key]}"; return; fi
    if quiet; then echo ""; return; fi
    if has_dialog; then
        out=$(whiptail --title "$TITLE" --backtitle "Étape $STEP sur $STEPS" --passwordbox "$q" 10 74 3>&1 1>&2 2>&3) || out=""
    else
        read -r -s -p "  $q " out </dev/tty || true
        echo >&3
    fi
    echo "$out"
}

# yesno KEY "question" défaut(oui|non) : prints oui or non.
yesno() {
    local key="$1" q="$2" def="${3:-oui}"
    if [ -n "${A[$key]+x}" ]; then echo "${A[$key]}"; return; fi
    if quiet; then echo "$def"; return; fi
    if has_dialog; then
        local flag=()
        [ "$def" = non ] && flag=(--defaultno)
        if whiptail --title "$TITLE" --backtitle "Étape $STEP sur $STEPS" "${flag[@]}" --yes-button "Oui" --no-button "Non" --yesno "$q" 12 74; then echo oui; else echo non; fi
    else
        local r
        read -r -p "  $q (oui/non) [$def] " r </dev/tty || true
        r="${r:-$def}"
        case "$r" in o*|O*|y*|Y*) echo oui ;; *) echo non ;; esac
    fi
}

# choose KEY "question" défaut clé1 "libellé 1" clé2 "libellé 2"… : prints the key.
choose() {
    local key="$1" q="$2" def="$3"
    shift 3
    if [ -n "${A[$key]+x}" ]; then echo "${A[$key]}"; return; fi
    if quiet; then echo "$def"; return; fi
    if has_dialog; then
        local out
        out=$(whiptail --title "$TITLE" --backtitle "Étape $STEP sur $STEPS" --default-item "$def" --menu "$q" 18 74 8 "$@" 3>&1 1>&2 2>&3) || out="$def"
        echo "$out"
    else
        local i=1 keys=()
        echo "  $q" >&3
        while [ $# -gt 0 ]; do echo "    $i) $2" >&3; keys+=("$1"); shift 2; i=$((i + 1)); done
        local r
        read -r -p "  Votre choix [$def] " r </dev/tty || true
        if [[ "$r" =~ ^[0-9]+$ ]] && [ "$r" -ge 1 ] && [ "$r" -le "${#keys[@]}" ]; then echo "${keys[$((r - 1))]}"; else echo "$def"; fi
    fi
}

info() {
    if has_dialog; then whiptail --title "$TITLE" --backtitle "Étape $STEP sur $STEPS" --msgbox "$1" 16 74; else echo "$1" | sed 's/^/  /' >&3; fi
}

# The settings, read and written with Python (installed on Ubuntu Server).
setting() { # setting chemin.json valeur-json
    python3 - "$SETTINGS" "$1" "$2" <<'PY'
import json, sys
path, key, value = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path) as f:
    s = json.load(f)
o = s
parts = key.split('.')
for p in parts[:-1]:
    o = o[int(p)] if isinstance(o, list) else o.setdefault(p, {})
last = parts[-1]
v = json.loads(value)
if isinstance(o, list):
    o[int(last)] = v
else:
    o[last] = v
tmp = path + '.tmp'
with open(tmp, 'w') as f:
    json.dump(s, f, ensure_ascii=False, indent=2)
import os
os.replace(tmp, path)
PY
}
current() { # current chemin.json : the value in place, as text
    python3 - "$SETTINGS" "$1" <<'PY' 2>/dev/null || true
import json, sys
s = json.load(open(sys.argv[1]))
o = s
for p in sys.argv[2].split('.'):
    o = o[int(p)] if isinstance(o, list) else o.get(p)
    if o is None:
        break
print('' if o is None else (o if isinstance(o, str) else json.dumps(o)))
PY
}
js() { python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1"; }

systemd_up() { [ -d /run/systemd/system ]; }

# ---------- 1. the machine ----------

step "La machine"
# Read in a subshell: /etc/os-release defines VERSION, NAME… of its own.
OS_VERSION=$(. /etc/os-release && echo "${VERSION_ID:-}")
case "$OS_VERSION" in
    22.04|24.04) say "Ubuntu $OS_VERSION : pris en charge." ;;
    *) say "Ubuntu ${OS_VERSION:-?} : non testé (22.04 et 24.04 le sont). On continue." ;;
esac
[ "$(dpkg --print-architecture)" = amd64 ] || fail "seule l'architecture amd64 est fournie"
FREE=$(df -Pk /var/lib | awk 'NR==2 {print int($4/1024/1024)}')
say "Place libre pour les tournages : ${FREE} Go (/var/lib)."
if [ "$FREE" -lt 100 ]; then
    # 6 channels in 1080p50 take about 135 Go an hour, before the copy.
    say "Attention : c'est peu. Une heure en 6 canaux 1080p50 prend environ 135 Go ; prévoyez un disque dédié aux tournages."
fi

# ---------- 2. the packages ----------

step "Les paquets nécessaires"
say "Mise à jour de la liste des paquets…"
export DEBIAN_FRONTEND=noninteractive
run apt-get update || fail "apt-get update (voir $LOG)"
say "Installation : ffmpeg, cifs-utils (partages SMB), python3, curl, whiptail…"
run apt-get install -y --no-install-recommends ffmpeg cifs-utils python3 curl ca-certificates whiptail iproute2 pciutils \
    || fail "installation des paquets (voir $LOG)"
if [ "$(yesno upgrade "Mettre aussi à jour tout le système (apt full-upgrade) ?" non)" = oui ]; then
    say "Mise à jour du système…"
    run apt-get -y full-upgrade || say "La mise à jour du système a échoué (voir $LOG) ; on continue."
fi
info "Bienvenue dans l'installation de Strata Studio.

Les questions qui suivent règlent le studio : mélangeur, console, caméras, nodal, micros, diffusion, partages. Ce que vous laissez vide reste « non réglé » : Studio le dit, et vous le réglez plus tard dans l'administration (http://<serveur>:8095/admin). Studio ne simule aucun appareil."

# ---------- 3. Strata Studio ----------

step "Strata Studio"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
if [ -z "$DEB" ]; then
    if [ -z "$VERSION" ]; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tag_name"].lstrip("v"))') \
            || fail "impossible de connaître la dernière version (accès à github.com ?)"
    fi
    say "Téléchargement de Strata Studio $VERSION…"
    BASE="https://github.com/$REPO/releases/download/v$VERSION"
    DEB="$TMP/strata-studio_${VERSION}_amd64.deb"
    curl -fsSL -o "$DEB" "$BASE/strata-studio_${VERSION}_amd64.deb" || fail "téléchargement du paquet"
    curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS" || fail "téléchargement des sommes"
    (cd "$TMP" && sha256sum -c --ignore-missing SHA256SUMS >> "$LOG" 2>&1) || fail "le paquet téléchargé ne correspond pas à sa somme SHA-256"
    say "Paquet vérifié (SHA-256)."
fi
[ -f "$DEB" ] || fail "paquet introuvable : $DEB"
run apt-get install -y "$(readlink -f "$DEB")" || fail "installation du paquet (voir $LOG)"
grep -q '^STRATA_STUDIO_DATA=' "$ETC/strata-studio.env" 2>/dev/null || echo "STRATA_STUDIO_DATA=$DATA" >> "$ETC/strata-studio.env"
say "Strata Studio est installé."
# A first start writes the settings, or brings older ones up to date.
install -d -o strata-studio -g strata-studio "$DATA"
(cd "$DATA" && runuser -u strata-studio -- timeout 3 /usr/bin/strata-studio --data "$DATA" --listen 127.0.0.1:0 >> "$LOG" 2>&1 || true)
[ -f "$SETTINGS" ] || fail "les réglages par défaut n'ont pas été créés (voir $LOG)"
systemd_up && systemctl stop strata-studio >> "$LOG" 2>&1 || true

# ---------- 4. the studio ----------

step "Le studio"
NAME=$(ask studio "Le nom du studio, tel qu'il s'affiche partout :" "$(current studio)")
setting studio "$(js "$NAME")"

# ---------- 5. switcher and console ----------

step "Le mélangeur et la console"
SW=$(choose switcher "Le mélangeur :" "$(k=$(current switcher.kind); echo "${k:-later}")" later "Plus tard" atem "Blackmagic ATEM" vmix "vMix")
[ "$SW" = later ] && SW=""
setting switcher.kind "$(js "$SW")"
if [ -n "$SW" ]; then
    setting switcher.address "$(js "$(ask switcher_address "Adresse IP du mélangeur :" "$(current switcher.address)")")"
fi
if [ "$SW" = vmix ]; then
    setting switcher.vmixSplit "$(ask vmix_split "vMix : numéro de l'entrée de mise en page « 50/50 » (deux couches) :" "$(current switcher.vmixSplit)")"
    setting switcher.vmixPip "$(ask vmix_pip "vMix : numéro de l'entrée de mise en page « vignette » :" "$(current switcher.vmixPip)")"
fi
CO=$(choose console "La console audio :" "$(k=$(current console.kind); echo "${k:-later}")" later "Plus tard" m32 "Midas M32" dm5 "Yamaha DM5")
[ "$CO" = later ] && CO=""
setting console.kind "$(js "$CO")"
[ -n "$CO" ] && setting console.address "$(js "$(ask console_address "Adresse IP de la console :" "$(current console.address)")")"

# ---------- 6. cameras ----------

step "Les caméras"
CAM=$(choose cameras "Le pilotage des caméras PTZ (Sony FR7, BRC-AM7) :" "$(k=$(current cameras.control); echo "${k:-later}")" later "Plus tard" visca "VISCA over IP")
[ "$CAM" = later ] && CAM=""
setting cameras.control "$(js "$CAM")"
if [ "$CAM" = visca ]; then
    for i in 0 1 2 3; do
        a=$(ask "camera$((i + 1))" "Adresse IP de la caméra $((i + 1)) :" "$(current setup.cameras.$i.address)")
        setting "setup.cameras.$i.address" "$(js "$a")"
    done
fi

# ---------- 7. nodal ----------

step "Le nodal"
RT=$(choose router "Le nodal vidéo :" "$(current router.kind)" none "Pas de nodal" videohub "Blackmagic Videohub (SDI)" nmos "NMOS IS-05 (ST 2110)" ndi "NDI (Studio route lui-même)")
setting router.kind "$(js "$RT")"
[ "$RT" = videohub ] && setting router.address "$(js "$(ask videohub "Adresse IP du Videohub :" "$(current router.address)")")"
[ "$RT" != none ] && setting router.pgmSource "$(js "$(ask pgm_source "Le nom de la source qui porte le programme du mélangeur :" "$(current router.pgmSource)")")"
if [ "$RT" = nmos ]; then
    info "NMOS IS-05 : déclarez les émetteurs et les récepteurs (nom, adresse de l'API Connection, identifiant) dans $SETTINGS, rubrique « router ». L'administration les affiche ensuite."
fi
NDI_WANTED=non
[ "$RT" = ndi ] && NDI_WANTED=oui
if [ "$(yesno ndi "Installer le runtime NDI 6 (nécessaire pour le routage NDI) ?

Il est téléchargé chez Vizrt (ndi.video) ; son installation vaut acceptation de la licence NDI (https://ndi.video/sdk-license)." "$NDI_WANTED")" = oui ]; then
    say "Téléchargement du SDK NDI…"
    if curl -fsSL -o "$TMP/ndi.tar.gz" "https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz" && tar -xzf "$TMP/ndi.tar.gz" -C "$TMP"; then
        (cd "$TMP" && echo y | PAGER=cat bash ./Install_NDI_SDK_v6_Linux.sh >> "$LOG" 2>&1) || true
        LIB=$(find "$TMP" -path '*x86_64-linux-gnu*' -name 'libndi.so.6*' -type f | head -1)
        if [ -n "$LIB" ]; then
            install -m 0644 "$LIB" /usr/local/lib/
            ln -sf "/usr/local/lib/$(basename "$LIB")" /usr/local/lib/libndi.so.6
            ldconfig
            say "Runtime NDI installé."
        else
            say "Le runtime NDI n'a pas été trouvé dans le SDK (voir $LOG)."
        fi
    else
        say "Téléchargement du SDK NDI impossible ; à installer plus tard."
    fi
fi

# ---------- 8. microphones and listening ----------

step "Les micros et l'écoute de l'animateur"
SDP=$(ask aes67_sdp "Le fichier SDP du flux AES67 des micros (Dante), vide : plus tard :" "$(current audio.sdp)")
setting audio.sdp "$(js "$SDP")"
if [ -n "$SDP" ]; then
    IFACES=$(ip -o -4 addr show | awk '{print $2 " " $4}' | tr '\n' ' ')
    setting audio.interface "$(js "$(ask aes67_if "L'adresse IP de la carte réseau Dante de ce serveur ($IFACES) :" "$(current audio.interface)")")"
fi
if [ "$(yesno whisper "Installer l'écoute de l'animateur (whisper.cpp, compilé sur ce serveur, quelques minutes) ?" non)" = oui ]; then
    MODEL=$(choose whisper_model "Le modèle de reconnaissance vocale :" small base "base (léger, rapide)" small "small (plus juste, conseillé avec une carte graphique)" medium "medium (le plus juste, lourd)")
    say "Compilation de whisper.cpp…"
    run apt-get install -y --no-install-recommends build-essential cmake git || fail "outils de compilation"
    if [ -d /opt/whisper.cpp/.git ]; then run git -C /opt/whisper.cpp pull --ff-only || true; else run git clone --depth 1 https://github.com/ggml-org/whisper.cpp /opt/whisper.cpp || fail "téléchargement de whisper.cpp"; fi
    run cmake -S /opt/whisper.cpp -B /opt/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release || fail "configuration de whisper.cpp"
    run cmake --build /opt/whisper.cpp/build -j "$(nproc)" --config Release --target whisper-server || fail "compilation de whisper.cpp"
    run sh /opt/whisper.cpp/models/download-ggml-model.sh "$MODEL" || fail "téléchargement du modèle $MODEL"
    cat > /etc/systemd/system/strata-whisper.service <<EOF
[Unit]
Description=Strata Studio, l'écoute de l'animateur (whisper.cpp)
After=network.target

[Service]
User=strata-studio
ExecStart=/opt/whisper.cpp/build/bin/whisper-server -m /opt/whisper.cpp/models/ggml-$MODEL.bin --host 127.0.0.1 --port 8178 -l fr -t $(nproc)
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF
    if systemd_up; then run systemctl daemon-reload; run systemctl enable --now strata-whisper.service || say "strata-whisper ne démarre pas (voir journalctl -u strata-whisper)"; fi
    setting audio.whisper '"127.0.0.1:8178"'
    say "L'écoute de l'animateur est installée (modèle $MODEL)."
fi

# ---------- 9. Buttons and the Stream Deck ----------

step "Bitfocus Buttons et le Stream Deck"
BT=$(ask buttons "L'adresse de Bitfocus Buttons (adresse:port, vide : plus tard) :" "$(current buttons.address)")
setting buttons.address "$(js "$BT")"
if [ -n "$BT" ]; then
    [ "$(yesno deck "Piloter aussi le parcours depuis un Stream Deck + branché sur Buttons ?" oui)" = oui ] && setting buttons.deck true || setting buttons.deck false
fi

# ---------- 10. the pictures ----------

step "Les images reçues (enregistrement, aperçus, diffusion)"
SRC=$(choose sources "D'où viennent les images du mélangeur et des caméras ?" later later "Plus tard" srt "SRT ou RTP (adresses réseau)" decklink "Cartes DeckLink (ffmpeg compilé avec DeckLink)")
if [ "$SRC" != later ]; then
    if [ "$SRC" = decklink ]; then
        FF=$(ask ffmpeg "Le chemin d'un ffmpeg compilé avec DeckLink (celui d'Ubuntu ne l'a pas) :" "/usr/local/bin/ffmpeg")
        sed -i "s|^STRATA_STUDIO_ARGS=.*|STRATA_STUDIO_ARGS=--ffmpeg $FF|" "$ETC/strata-studio.env"
    fi
    i=0
    for ch in PGM CLEAN CAM1 CAM2 CAM3 CAM4; do
        if [ "$SRC" = srt ]; then
            u=$(ask "input_$ch" "Adresse du flux $ch (srt://… ou rtp://…) :" "srt://0.0.0.0:$((9000 + i))?mode=listener")
            arr=$(python3 -c 'import json,sys; print(json.dumps(["-i", sys.argv[1]]))' "$u")
        else
            d=$(ask "input_$ch" "Nom de l'entrée DeckLink pour $ch :" "DeckLink 8K Pro ($((i + 1)))")
            arr=$(python3 -c 'import json,sys; print(json.dumps(["-f", "decklink", "-i", sys.argv[1]]))' "$d")
        fi
        setting "recording.channels.$i.input" "$arr"
        i=$((i + 1))
    done
fi
CODEC=$(choose codec "L'encodage des fichiers (4:2:2) :" "$(current recording.codec)" h264 "H.264" h265 "H.265")
setting recording.codec "$(js "$CODEC")"
CONT=$(choose container "Le conteneur :" "$(current recording.container)" mov "MOV" mp4 "MP4")
setting recording.container "$(js "$CONT")"

# ---------- 11. the graphics card ----------

step "La carte graphique"
GPU=$(lspci 2>/dev/null | grep -Ei 'vga|3d|display' || true)
say "${GPU:-Aucune carte graphique détectée.}"
ACCEL=cpu
if echo "$GPU" | grep -qi nvidia; then
    if [ "$(yesno nvidia "Une carte NVIDIA est là. Installer son pilote (ubuntu-drivers) ?" oui)" = oui ]; then
        run apt-get install -y ubuntu-drivers-common && run ubuntu-drivers install || say "Le pilote NVIDIA ne s'est pas installé (voir $LOG)."
        say "Pilote NVIDIA installé : un redémarrage du serveur sera nécessaire."
    fi
    ACCEL=nvenc
elif echo "$GPU" | grep -qi intel; then
    if [ "$(yesno intel "Un processeur graphique Intel est là. Installer Quick Sync (pilotes média Intel) ?" oui)" = oui ]; then
        run apt-get install -y intel-media-va-driver-non-free libvpl2 || run apt-get install -y intel-media-va-driver || say "Pilotes Intel non installés (voir $LOG)."
    fi
    ACCEL=qsv
fi
ACCEL=$(choose accel "L'encodeur à utiliser :" "$ACCEL" cpu "Le processeur (lent : un canal 1080p50 sur 4 cœurs)" nvenc "NVIDIA (4:2:2 : carte Blackwell et ffmpeg récent)" qsv "Intel Quick Sync (H.265 4:2:2 seulement)")
setting recording.accel "$(js "$ACCEL")"
setting live.accel "$(js "$ACCEL")"
[ "$ACCEL" = qsv ] && setting recording.codec '"h265"'
say "Mesurer ce que la machine tient : banc-encodage --res 2160 --codec h265 --accel $ACCEL --max 8"

# ---------- 12. live streaming ----------

step "La diffusion"
GOS="[]"
for i in 1 2 3 4; do
    a=$(ask "go$i" "Adresse du Blackmagic Streaming Encoder de GO$i (vide : pas de GO$i) :" "$(current gos.$((i - 1)).address)")
    [ -n "$a" ] && GOS=$(python3 -c 'import json,sys; l=json.loads(sys.argv[1]); l.append({"label": "GO"+sys.argv[2], "address": sys.argv[3], "destination": "GO"+sys.argv[2]}); print(json.dumps(l))' "$GOS" "$i" "$a")
done
setting gos "$GOS"
YK=$(secret youtube_key "La clé de diffusion YouTube (vide : plus tard) :")
[ -n "$YK" ] && setting live.youtube.key "$(js "$YK")"
TU=$(ask tiktok_url "Le serveur de diffusion TikTok (rtmp://…, vide : plus tard) :" "$(current live.tiktok.url)")
setting live.tiktok.url "$(js "$TU")"
TK=$(secret tiktok_key "La clé de diffusion TikTok (vide : plus tard) :")
[ -n "$TK" ] && setting live.tiktok.key "$(js "$TK")"
QA=$(ask quicklink_api "L'adresse de l'API Quicklink (StudioEdge, calls), vide : plus tard :" "$(current calls.api)")
setting calls.api "$(js "$QA")"
if [ -n "$QA" ]; then
    setting calls.username "$(js "$(ask quicklink_user "Compte Quicklink :" "$(current calls.username)")")"
    QP=$(secret quicklink_password "Mot de passe Quicklink :")
    [ -n "$QP" ] && setting calls.password "$(js "$QP")"
    setting calls.room "$(js "$(ask quicklink_room "La salle de distribution de StudioEdge :" "$(current calls.room)")")"
fi
chmod 640 "$SETTINGS"
chown strata-studio:strata-studio "$SETTINGS"

# ---------- 13. the shares ----------

step "Les partages où copier les tournages"
STORES="[]"
n=0
while :; do
    n=$((n + 1))
    if [ "$n" -gt 1 ] || [ -z "${A[share1]+x}" ]; then
        more=$(yesno "share_more$n" "Ajouter un partage SMB (NAS, Windows) où copier les tournages ?" "$([ "$n" = 1 ] && echo oui || echo non)")
        [ "$more" = oui ] || break
    fi
    SHARE=$(ask "share$n" "Le partage, sous la forme //serveur/partage :" "")
    [ -n "$SHARE" ] || break
    LABEL=$(ask "share${n}_label" "Son nom sur la tablette :" "$(basename "$SHARE")")
    MNT="/mnt/$(python3 -c 'import re, sys, unicodedata; t = unicodedata.normalize("NFKD", sys.argv[1]).encode("ascii", "ignore").decode().lower(); print(re.sub(r"[^a-z0-9]+", "-", t).strip("-") or "partage")' "$LABEL")"
    USER_=$(ask "share${n}_user" "Le compte du partage :" "")
    PASS=$(secret "share${n}_password" "Le mot de passe du partage :")
    DOM=$(ask "share${n}_domain" "Le domaine (vide s'il n'y en a pas) :" "")
    install -d -m 700 "$ETC/smb"
    CRED="$ETC/smb/$(basename "$MNT")"
    { echo "username=$USER_"; echo "password=$PASS"; [ -n "$DOM" ] && echo "domain=$DOM"; } > "$CRED"
    chmod 600 "$CRED"
    install -d "$MNT"
    LINE="$SHARE  $MNT  cifs  credentials=$CRED,uid=strata-studio,gid=strata-studio,iocharset=utf8,_netdev,nofail  0  0"
    grep -vF " $MNT " /etc/fstab > "$TMP/fstab" || true
    echo "$LINE" >> "$TMP/fstab"
    cat "$TMP/fstab" > /etc/fstab
    if mount "$MNT" >> "$LOG" 2>&1 || mountpoint -q "$MNT"; then say "$LABEL monté sur $MNT."; else say "$LABEL n'a pas pu être monté maintenant (voir $LOG) ; il le sera au démarrage."; fi
    STORES=$(python3 -c 'import json,sys; l=json.loads(sys.argv[1]); l.append({"label": sys.argv[2], "path": sys.argv[3]}); print(json.dumps(l, ensure_ascii=False))' "$STORES" "$LABEL" "$MNT")
done
if [ "$STORES" != "[]" ]; then setting storages "$STORES"; fi

# ---------- 14. firewall and start ----------

step "Le pare-feu et le démarrage"
chown strata-studio:strata-studio "$SETTINGS"
chmod 640 "$SETTINGS"
if command -v ufw >/dev/null 2>&1 && ufw status 2>/dev/null | grep -q "Status: active"; then
    run ufw allow 8095/tcp; run ufw allow 5004/udp; run ufw allow 9875/udp
    say "Pare-feu : ports 8095/tcp (Studio), 5004 et 9875/udp (AES67) ouverts."
else
    say "Pare-feu ufw inactif : rien à ouvrir."
fi
IP=$(ip -o -4 route get 1.1.1.1 2>/dev/null | awk '{for (i=1;i<=NF;i++) if ($i=="src") print $(i+1)}')
IP=${IP:-$(hostname -I 2>/dev/null | awk '{print $1}')}
if systemd_up; then
    run systemctl daemon-reload
    run systemctl enable --now strata-studio.service
    run systemctl restart strata-studio.service
    for _ in $(seq 1 30); do curl -fsS "http://127.0.0.1:8095/api/health" >/dev/null 2>&1 && break; sleep 1; done
    if curl -fsS "http://127.0.0.1:8095/api/health" >/dev/null 2>&1; then say "Strata Studio répond."; else say "Strata Studio ne répond pas : journalctl -u strata-studio"; fi
else
    say "systemd absent (conteneur ?) : lancez Studio avec  strata-studio --data $DATA"
fi
info "Strata Studio est installé.

  Tablette et téléphone :  http://${IP:-<adresse>}:8095/
  Écran du studio :        http://${IP:-<adresse>}:8095/ecran
  Administration :         http://${IP:-<adresse>}:8095/admin/systeme

Réglages : $SETTINGS
Journal de l'installation : $LOG
Journal du service : journalctl -u strata-studio -f"
echo >&3
say "Terminé."
