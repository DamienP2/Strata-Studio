# Strata Studio

**Le studio autonome.** Strata Studio automatise un studio de podcast ou de radio filmée. Un journaliste prépare et lance son émission depuis une tablette. Une IA locale commute le mélangeur (ATEM ou vMix) et cadre les caméras PTZ (Sony FR7, BRC-AM7). Un réalisateur peut reprendre la main à tout moment.

Studio est un système de contrôle. Il pilote le nodal (Videohub en SDI, NMOS IS-05 en ST 2110, routage NDI), le mélangeur, les caméras et la console audio (Midas M32, Yamaha DM5). Il passe par Bitfocus Buttons pour la lumière et les automations. Aucune image ne passe par lui.

Strata Studio fait partie de la galaxie Strata, avec Strata Graphics et Strata Multiview.

> État : **étape 0**. Le Réalisateur et un studio simulé fonctionnent ; les pilotes des appareils réels sont à écrire, sauf la console M32. Les fonctionnalités sont dans [`docs/FONCTIONNALITES.md`](docs/FONCTIONNALITES.md), l'architecture dans [`docs/PROPOSITION.md`](docs/PROPOSITION.md), les maquettes sur https://claude.ai/artifact/U6a7FSWuRyNGfwNzkCqmrG.

## Essayer

```bash
cargo run -p strata-studio-server
```

Le serveur démarre un **studio simulé** : 5 places, 4 PTZ, un mélangeur, une console et une conversation de démonstration. Le Réalisateur le dirige en direct.

Pour piloter un vrai mélangeur à la place du simulé :

```bash
cargo run -p strata-studio-server -- --atem 192.168.10.2
```

```bash
cargo run -p strata-studio-server -- --vmix 192.168.10.3 --vmix-split 20 --vmix-pip 21
```

Avec l'ATEM, le 50/50 et la vignette sont des mises en page de la SuperSource (entrée 6000), dont Studio règle les cases. Avec vMix, ce sont deux entrées de mise en page à deux couches, préparées dans vMix (`--vmix-split`, `--vmix-pip`). Une coupe faite à la main au pupitre de l'ATEM ou dans vMix suspend l'IA.

```bash
curl -s localhost:8095/api/state
```

La reprise en main :

```bash
curl -X POST "localhost:8095/api/control/take?by=Paul"
```

```bash
curl -X POST localhost:8095/api/control/auto
```

```bash
curl -X POST "localhost:8095/api/control/force/100?seconds=10"
```

```bash
curl -X POST localhost:8095/api/demo/panel-cut/2
```

La dernière commande simule une personne qui coupe sur le pupitre du mélangeur : l'IA se suspend d'elle-même.

L'écoute de l'animateur, comme si la reconnaissance vocale avait entendu sa phrase :

```bash
curl -X POST localhost:8095/api/demo/say -H 'content-type: application/json' -d '{"text": "On retrouve tout de suite Julie en duplex, bonjour Julie !"}'
```

```bash
curl -X POST localhost:8095/api/demo/remote/2/voice/1
```

Le journal (`/api/state`) montre l'intention comprise, le 50/50 préparé puis pris à la fin de la phrase, et le plein cadre quand Julie parle longtemps.

## Code

| Crate | Rôle |
| --- | --- |
| `strata-studio-core` | le studio (places, caméras, plans, sources extérieures), la perception, les profils, le **Réalisateur** (fonction pure, testée sur des conversations scriptées) et l'**écoute** de l'animateur (des intentions tirées de ses phrases) |
| `strata-studio-devices` | les interfaces des appareils (mélangeur, nodal, PTZ, console, Bitfocus), leurs simulateurs, la console Midas M32 (OSC) les caméras Sony en VISCA over IP, et les mélangeurs **ATEM** (UDP 9910 : coupe, SuperSource, macros, lecture du programme) et **vMix** (TCP 8099 : coupe, couches des mises en page, tally) |
| `strata-studio-sense` | l'oreille de l'animateur : son micro découpé en phrases, transcrites par un serveur whisper.cpp local, sans les textes que Whisper invente dans le silence ; l'exemple `ecoute` l'essaie sur un enregistrement |
| `strata-studio-bench` | `banc-encodage` : combien de canaux 4:2:2 (H.264, H.265 ; 1080p50, 2160p50) la machine encode en temps réel |
| `strata-studio-server` | l'exécutable `strata-studio` : studio simulé, API d'état et de reprise en main |

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## Essayer l'écoute sur un enregistrement

Il faut un serveur whisper.cpp sur la machine, avec un modèle multilingue (`ggml-base.bin` ou plus gros) :

```bash
whisper-server -m models/ggml-base.bin --host 127.0.0.1 --port 8178 -l fr
```

Puis, sur un enregistrement du micro de l'animateur (WAV 16 bits) :

```bash
cargo run -p strata-studio-sense --example ecoute -- enregistrement.wav
```

Chaque phrase entendue s'affiche, avec ce que le Réalisateur en comprend.

## Mesurer l'encodage de la machine

Combien de canaux 4:2:2 la machine tient en temps réel (à lancer sur le serveur retenu) :

```bash
cargo run --release -p strata-studio-bench -- --res 2160 --codec h265 --accel nvenc --max 8
```

`--accel` : `cpu`, `nvenc` (NVIDIA ; le 4:2:2 demande une carte Blackwell et un FFmpeg récent) ou `qsv` (Intel, H.265 4:2:2 10 bits). Le verdict dit combien de canaux tiennent avec 10 % de marge ; Studio en demande jusqu'à 6, plus la diffusion.
