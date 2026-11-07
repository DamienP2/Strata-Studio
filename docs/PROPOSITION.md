# Strata Studio : proposition (v5)

> Statut : **proposition**. Le Réalisateur (règles, décisions, reprise en main) et le studio simulé existent ; les pilotes réels sont à écrire, sauf la console Midas M32.

**Strata Studio** rend un studio de podcast ou de radio filmée **autonome**. Son cœur est une **IA locale de réalisation et de mixage** : elle commute le mélangeur, cadre les caméras PTZ et, si besoin, tient le mélange des micros. Un journaliste suit et contrôle son émission depuis une **tablette en Wi-Fi**, étape par étape. Il n'y a pas de réalisateur ; quelqu'un peut **reprendre la main directement sur le mélangeur ou sur la console**.

Studio est un **système de contrôle**. Il pilote le nodal, le mélangeur, les caméras, la console audio et, par Bitfocus, la lumière et le reste. **Aucune image ne passe par lui** : s'il s'arrête, le studio reste dans son dernier état et l'antenne continue.

---

## 0. Les décisions prises

| Sujet | Décision |
| --- | --- |
| Réalisation | **automatique** par une IA interne et locale : c'est la priorité du projet |
| Mixage audio | **par l'IA si besoin**, au travers de la console (§ 5.6) |
| Reprise en main | **directement sur le mélangeur et/ou sur la console audio** ; pas d'écran de régie |
| Mélangeur | **tous les ATEM** et **vMix (version actuelle)**, pilotés (pas de mélangeur interne) |
| Formats | **SDI, NDI et ST 2110** |
| Nodal | **Videohub** en SDI ; en ST 2110, **pas de registre NMOS** ; en NDI, **pas de routeur** : Studio route lui-même |
| Direct et enregistré | les deux, **au travers d'un nodal piloté par Studio** |
| Caméras | 4 PTZ **Sony FR7 ou BRC-AM7** |
| Intervenants | jusqu'à **5** |
| Micros | **Dante**, au travers d'une console **Midas M32** ou **Yamaha DM5** (même protocole que les DM3 et DM7), pilotée par Studio |
| Lumière | **DMX via Bitfocus Buttons** |
| Automations | **Bitfocus Buttons seulement**, qui gère tout le reste ; dans les deux sens |
| Image pour l'IA | la **sortie NDI** des FR7 et AM7 |
| Destinations (direct, fichiers) | **à définir** |
| Tablette | **contrôle et surveillance** (enregistrement, direct, son, image, lumière), **étape par étape avec vérification** ; aucune identification |
| Administration | **un canvas de nœuds** relié à Buttons, qui fait les contrôles |
| OnAir Studio | **non** |
| Code | **dépôt à part** (celui-ci), sur le modèle de Strata Multiview |

---

## 1. Le studio piloté

```
                          ┌──────────────── Strata Studio (serveur, GPU) ────────────────┐
  Tablette Wi-Fi ───WS───►│  Scénarios (canvas de nœuds) · IA de réalisation et de mixage│
  (journaliste)           │  Pilotes : mélangeur · nodal · PTZ · console audio           │
                          └───┬─────────┬──────────┬───────────┬─────────────┬───────────┘
              contrôle seul   │         │          │           │             │  HTTP · OSC · TCP
                     ┌────────▼──┐ ┌────▼────┐ ┌───▼─────┐ ┌───▼────────┐ ┌──▼──────────────────────┐
                     │   Nodal   │ │ ATEM /  │ │ 4 PTZ   │ │ Console    │ │ Bitfocus Buttons        │
                     │ SDI · NDI │ │ vMix    │ │ FR7·AM7 │ │ M32 · DM5  │ │ lumière DMX, ON AIR,    │
                     │ ST 2110   │ │         │ │         │ │            │ │ automations, Stream Deck│
                     └───────────┘ └─────────┘ └─────────┘ └────────────┘ └─────────────────────────┘
  signal :  caméras ──► nodal ──► mélangeur ──► nodal ──► diffusion, encodeurs, enregistreurs
            micros Dante ──► console ──► programme
  analyse : copie AES67 des micros ──► Studio      flux réduit de chaque caméra ──► Studio
  reprise : une main sur le mélangeur ou sur la console suffit
```

---

## 2. Ce que Studio pilote lui-même

Studio pilote **directement** ce dont l'IA a besoin vite et avec un retour d'état. Le reste passe par Bitfocus (§ 3).

| Appareil | Protocole | Ce que Studio fait | Base existante |
| --- | --- | --- | --- |
| **ATEM** | UDP 9910 | programme et preview, cut et mix, macros, SuperSource, keyers, aux, enregistrement et stream (modèles ISO) ; lit le tally et **voit une action humaine sur le pupitre** | client ATEM de Strata Multiview (`tally/atem.rs`, en lecture : à étendre aux commandes) |
| **vMix** | API TCP 8099 | cut, fondu, inputs, overlays, enregistrement, stream ; état et tally | à écrire |
| **Nodal SDI** | Videohub (TCP 9990, protocole texte) | routes par émission (salves) : PGM vers la diffusion et les enregistreurs, retours ; noms des entrées et sorties ; verrouillage des sorties sensibles | à écrire |
| **Nodal ST 2110** | NMOS IS-05, **en direct vers chaque appareil** (pas de registre : leurs adresses sont déclarées sur la page Plateau, ou trouvées en mDNS) ; état PTP | mêmes routes en IP ; vérification de la synchro avant antenne | IS-05 dans Strata Multiview, état PTP dans Strata Graphics (`sync.rs`) |
| **NDI** | routage NDI du SDK : **Studio fait office de routeur** (sources virtuelles qu'il redirige) | mêmes routes en NDI ; aucune image ne passe par Studio | runtime NDI déjà chargé par Strata Graphics |
| **Sony FR7 / BRC-AM7** | VISCA over IP, CGI HTTP ; OpenTrackIO (FR7) | presets, pan, tilt, zoom, vitesses, mise au point ; position réelle de la tête ; suivi automatique intégré si le modèle et le firmware l'ont (à vérifier) | OpenTrackIO dans Strata Graphics (`tracking.rs`) |
| **Midas M32** | OSC, UDP 10023 | scène de l'émission, **noms des invités sur les tranches**, ouverture et coupure des micros, faders ; retour des changements faits sur la console (`/xremote`) | **écrit** : `strata-studio-devices/src/m32.rs` (coupure, nom) |
| **Yamaha DM5** | RCP, TCP 49280 (le même protocole que les DM3 et DM7) | idem | à écrire |
| **Micros Dante** | AES67 (SAP, RTP L24 48 kHz) ou carte son Dante | copie de chaque micro pour l'IA ; présence du signal pour la vérification | `dante.rs` de Strata Multiview |
| **Habillage** | API de Strata Graphics | noms des invités, synthé de celui qui parle | API existante |
| **Multiview** | TSL 5.0 | UMD : « IA · Cam 2 · Alice », « MANUEL · Paul » | Strata Multiview lit le TSL |

### 2.1 Les trois formats

Le **pilote de nodal** est choisi par format : SDI (Videohub), ST 2110 (NMOS IS-05 sans registre) ou NDI (routage fait par Studio). Un même studio peut mélanger les trois. Les scénarios parlent de **destinations et de sources nommées** (« PGM → Diffusion 1 »), jamais de numéros de grille.

Pour l'**analyse**, l'IA lit la **sortie NDI** de chaque FR7 et AM7, décodée en basse définition, quel que soit le format du studio.

### 2.2 La console audio

Studio ne fait pas le mélange : la console le fait. Studio la **pilote** :

- **préparation** : rappel de la scène de l'émission, nom de chaque invité sur sa tranche, coupure des places vides ;
- **antenne** : ouverture des micros des invités présents ;
- **pendant** : une touche « toux » sur la tablette si le scénario la prévoit ;
- **fin** : coupure de tous les micros.

Le retour de la console sert aussi : un micro coupé à la main, un fader descendu, Studio le voit et l'IA en tient compte (on ne coupe pas sur quelqu'un dont le micro est fermé).

### 2.3 État voulu, état réel

Studio garde l'**état voulu** de chaque appareil (routes, scène, presets, micros ouverts, enregistrement) et le compare en continu à l'**état réel**.

- Un appareil qui redémarre ou se reconnecte retrouve l'état voulu.
- Une différence inattendue (route changée à la main, enregistreur arrêté) lève une alerte sur la tablette et dans la régie.
- Toutes les commandes sont **idempotentes** : une commande répétée parce que le Wi-Fi a hésité ne fait rien de plus.

---

## 3. Studio et Bitfocus

Bitfocus Buttons pilote déjà la lumière en DMX et les automations du studio. Studio ne le remplace pas : **ils travaillent ensemble, dans les deux sens**.

| Sens | Comment | Exemples |
| --- | --- | --- |
| **Studio → Bitfocus** | appui sur un bouton (page, ligne, colonne) ; écriture de variables | scène lumière « préparation », « antenne », « fin » ; voyant ON AIR ; toute automation déjà faite dans Buttons |
| **Bitfocus → Studio** | API REST courte, OSC, TCP texte (comme Strata Graphics : `/api/v1/…`, UDP 9000, TCP 9001) | lancer une étape du scénario, rendre la main à l'IA, verrouiller une caméra |
| **Retours vers Bitfocus** | variables tenues à jour par Studio | `studio_mode` (IA ou MANUEL), `studio_speaker` (qui parle), `studio_on_air`, `studio_recording`, plan à l'antenne |

Un Stream Deck en régie ou sur le plateau devient ainsi une **surface de Studio** sans rien développer de plus. Un module dédié pour Bitfocus pourra venir ensuite (actions, retours, variables prêtes à l'emploi).

### 3.1 L'administration : un canvas de nœuds

L'administration tient sur **un seul écran : un canvas de nœuds**, comme un graphe. On y décrit ce qui se passe à chaque étape de l'émission, en reliant des nœuds par des fils :

| Nœud | Exemples |
| --- | --- |
| **Déclencheur** | une étape de la tablette (préparer, vérifier, antenne, pause, terminer) ; un bouton de la tablette (toux, lumière) ; une heure ; une alerte (micro faible, direct coupé) ; un événement de l'IA (changement de locuteur) |
| **Condition** | direct ou enregistré ; place occupée ; profil de l'émission |
| **Action Buttons** | les boutons et les actions de Buttons, lus par la connexion à Buttons : scène lumière, ON AIR, routes, enregistreurs, tout ce que Buttons sait faire |
| **Action Studio** | ce que l'IA a besoin de faire vite et avec un retour : profil de réalisation, mixage, presets PTZ |
| **Surveillance** | ce que la tablette affiche et vérifie : un état lu dans Buttons ou sur un appareil (le stream tourne, l'enregistreur enregistre, le son arrive) |

Exemple : *Antenne* → *Buttons : lumière antenne* → *Buttons : ON AIR* → *Studio : ouvrir les micros des places occupées* → *Studio : IA en marche, profil Débat*.

Le canvas **remplace** les pages Plateau et Scénario de la version 1 : Studio y lit les appareils et les actions que Buttons connaît déjà, l'admin n'a qu'à les relier.

---

## 4. La tablette des journalistes

Une application web installable, servie par Studio en Wi-Fi, pour quelqu'un qui n'est **pas technicien**. **Pas d'identification** : on l'ouvre, on s'en sert.

Elle ne sert qu'à deux choses :

- **contrôler** : avancer d'étape, la lumière, la toux, la pause, terminer ;
- **surveiller**, en mots simples : l'enregistrement tourne, le direct est en ligne, chaque voix est bien entendue, les caméras suivent, la lumière est sur la bonne scène. Un problème s'affiche avec ce qu'il faut faire.

Elle ne montre ni adresse, ni numéro d'entrée, ni réglage technique.

### 4.1 Le déroulé

| Écran | Ce que fait le journaliste | Ce que Studio fait |
| --- | --- | --- |
| **Accueil** | choisit son émission | charge le scénario de l'émission |
| **Préparation** | place les noms des invités sur le plan du plateau (places 1 à 5) | lumière « préparation » (Bitfocus), presets PTZ, routes du nodal, scène et noms sur la console, noms vers Strata Graphics |
| **Vérification** | chaque invité dit une phrase | contrôle : chaque micro a du signal et la bonne personne est reconnue, les caméras répondent, l'enregistreur a de la place, le nodal, le mélangeur, la console et Bitfocus répondent, la synchro est bonne (2110). Tout est vert, ou la tablette dit quoi faire |
| **Antenne** | **ENREGISTRER** ou **DIRECT** | lumière « antenne », micros ouverts, enregistrements, routes de diffusion, ON AIR, chrono, IA en marche |
| **Pendant** | quelques boutons, si le scénario les prévoit : *Plan large*, *Toux*, *Pause* | l'IA en tient compte |
| **Fin** | **TERMINER** (appui long) | micros coupés, arrêt propre, lumière « fin », fichiers nommés et déposés, résumé |

Le **chrono d'antenne** est dans Studio et s'affiche sur la tablette.

### 4.2 La fiabilité en Wi-Fi

- La tablette est une **télécommande, pas un cerveau** : l'état vit dans Studio. Perdre la tablette ou le Wi-Fi n'arrête rien.
- Reconnexion automatique ; l'écran se recale sur l'état réel.
- Les actions irréversibles demandent un **appui long**.
- Plusieurs tablettes peuvent suivre le même studio.
- Secours : les mêmes actions sur un Stream Deck, par Bitfocus.

### 4.3 Sans identification, protégée par le réseau

Le Wi-Fi des tablettes est un réseau dédié qui **n'atteint que Studio**. Studio est le seul à parler aux appareils. Les pages d'administration et de régie restent protégées par compte, comme dans les autres applications Strata.

---

## 5. L'IA de commutation

### 5.1 Locale

Tout tourne **sur le serveur Studio**, avec sa carte NVIDIA. Aucun son, aucune image, aucun texte ne sort du studio. Les modèles tournent avec ONNX Runtime, comme le segmenteur de personnes de Strata Graphics (`people_net.rs`).

### 5.2 Trois couches

| Couche | Délai | Rôle | Modèles envisagés |
| --- | --- | --- | --- |
| **Perception** | 10 à 40 ms | qui parle : voix sur chaque micro (VAD), micro dominant malgré la repisse ; visages, lèvres, orientation de la tête sur chaque caméra ; locuteur actif en croisant son et image | Silero VAD (MIT), détection de visages MediaPipe (Apache 2.0), modèle de locuteur actif audio-vidéo |
| **Anticipation** | 100 à 500 ms | **qui va parler** : fin d'une question, regard vers un invité, prise de souffle | modèle de prise de parole, entraîné sur les émissions du studio |
| **Compréhension** | 0,5 à 2 s, optionnelle | une question adressée à quelqu'un par son nom, un rire général, un changement de sujet | transcription locale (Whisper) et petit modèle de langage local |

### 5.3 Pourquoi l'anticipation est centrale

Le signal passe par l'ATEM ou vMix : Studio **ne peut pas retarder l'image** pour couper au bon moment. Une IA qui attend la voix coupe toujours en retard. Elle doit donc **prédire** qui va parler, et une PTZ hors antenne doit **déjà être cadrée** sur cette personne. En **enregistré**, l'IA peut en plus refaire le montage après coup à partir des ISO.

### 5.4 Quatre caméras pour cinq personnes

L'IA **répartit les PTZ en continu**. Une caméra est à l'antenne ; les autres se cadrent sur les personnes les plus probables ensuite. Une caméra sans personne à attendre revient au plan large, ou au plan qui montre ceux qui parlent ensemble. **Une caméra à l'antenne ne bouge jamais.**

### 5.5 Des règles comme garde-fous, l'IA pour choisir

1. **Les règles** fixent ce qui est interdit ou obligatoire : durées minimale et maximale d'un plan, interjections ignorées, pas de plan en mouvement ni mal cadré, pas de caméra verrouillée, pas de plan interdit. Elles sont réglables par profil : *Podcast posé*, *Débat*, *Interview*, *Radio filmée*.
2. **L'IA classe les plans permis** à chaque image. Aujourd'hui, c'est une formule. Demain, ce sera un modèle appris sur les reprises en main.

Le Réalisateur est une **fonction pure** : une émission enregistrée se **rejoue** et donne les mêmes décisions. Chaque décision porte sa **raison** (« Alice parle », « plan tenu longtemps : plan de coupe », « Karim pourrait parler »).

### 5.6 Le mixage audio par l'IA

L'IA tient aussi le son quand personne n'est à la console. Elle **ne fait pas passer l'audio par Studio** : elle pilote la console, qui garde le chemin du signal.

Deux vitesses :

1. **Rapide (quelques millisecondes)** : le partage de gain entre les micros ouverts (automix), qui baisse les micros de ceux qui ne parlent pas. Il doit tourner **dans la console**, pas sur le réseau : piloter des faders en direct par OSC ou RCP ajouterait du retard et des à-coups. Studio active et règle l'automix de la console quand elle en a un (à vérifier selon le modèle et le firmware de la M32 et de la DM5) ; à défaut, il se limite à la vitesse lente.
2. **Lente (de la seconde à la minute)** : ce que l'IA décide elle-même :
   - ouvrir les micros des places occupées, fermer les autres ;
   - **niveau de chaque voix** : rapprocher les voix trop faibles ou trop fortes, en douceur ;
   - **niveau du programme** : viser une loudness cible (EBU R128, déjà mesurée par Strata Multiview) ;
   - **jingles et virgules** : baisser les voix le temps qu'ils passent ;
   - **alertes** : micro qui tombe, souffle, larsen, voix trop proche ; l'alerte part vers la tablette avec la consigne (« rapprochez le micro de Karim »).

Elle s'appuie sur la même perception que la réalisation (qui parle, à quel niveau) : son et image restent cohérents.

---

## 6. La reprise en main : sur le mélangeur et sur la console

Il n'y a **pas d'écran de régie**. On reprend la main là où on a l'habitude de travailler :

| Où | Ce que Studio voit | Effet |
| --- | --- | --- |
| **Mélangeur** (pupitre ATEM, interface vMix) | une coupe ou une transition qu'il n'a pas commandée | l'IA **cesse de commuter et de bouger les caméras** |
| **Console** (M32, DM5) | un fader, une coupure ou un réglage qu'il n'a pas commandé | l'IA **cesse de mixer** ; le reste du son est laissé tel quel |

Les deux sont **indépendants** : on peut reprendre l'image et laisser le son à l'IA, ou l'inverse.

**Rendre la main à l'IA** : par défaut, après un temps sans geste humain (réglable, par exemple 30 s), ou tout de suite par un bouton de Buttons. L'IA repart du plan et des niveaux en place, sans à-coup.

L'état est visible sur la tablette (« Image : à la main », « Son : automatique ») et dans les variables de Buttons.

**Mode ombre** : l'IA décide sans rien envoyer. On compare ses choix à ceux d'un réalisateur humain avant de lui confier l'antenne. Chaque reprise en main est gardée, en local, pour entraîner le classement des plans.

---

## 7. Architecture logicielle

```
crates/
  strata-studio-core      ✔ modèle (places, caméras, plans), perception, profils, Réalisateur (pur, testé)
  strata-studio-devices   ✔ interfaces (Switcher, Router, Ptz, AudioConsole, Automation), simulateurs, M32
                          → ATEM, vMix, Videohub, NMOS IS-05, routage NDI, VISCA, CGI Sony, Yamaha RCP, Bitfocus
  strata-studio-server    ✔ exécutable strata-studio : studio simulé, API d'état et de reprise en main
                          → scénarios, état voulu / réel, tablette, régie, comptes, journal
  strata-studio-sense     → AES67, VAD, dominance, vision, transcription
  strata-studio-tray      → appli de la zone de notification / barre des menus
web/                      → tablette ; canvas d'administration (nœuds) relié à Buttons
```

### Repris des autres applications Strata

| Brique | Source |
| --- | --- |
| comptes, SSO, service natif, mises à jour, performances, journaux, appli de la barre des menus | Strata Graphics |
| inférence ONNX sur GPU ou CPU, OpenTrackIO, état PTP et NTP | Strata Graphics |
| client ATEM, TSL, Dante / AES67, NMOS IS-05, Ember+, salves | Strata Multiview |
| API courte, OSC et TCP pour Bitfocus | Strata Graphics |

### Serveur

Une machine Linux avec une carte NVIDIA, sur le réseau de contrôle, le réseau Dante et, en 2110, le réseau PTP. Les tablettes arrivent par un Wi-Fi séparé qui ne voit que Studio. La puissance nécessaire sera mesurée à l'étape 0.

---

## 8. Étapes

**Étape 0 : le socle et l'IA en ombre** (commencée)
- ✔ Réalisateur : règles, classement, préparation des PTZ, détection d'une reprise au pupitre, rejeu déterministe.
- ✔ Studio simulé et API de reprise en main.
- ✔ Console Midas M32 : coupure et noms.
- Pilotes ATEM et vMix (cut, preview, macros, tally, détection d'une action humaine).
- Presets VISCA sur FR7 et AM7.
- Micros Dante en AES67, VAD et micro dominant.
- Mode ombre sur de vraies émissions, avec la mesure de l'accord entre l'IA et le réalisateur.
- Reprise en main vue sur le mélangeur et sur la console.

**Étape 1 : le studio autonome**
- Tablette de contrôle et de surveillance, étape par étape avec vérification.
- Canvas d'administration (nœuds) relié à Buttons.
- Mixage par l'IA (vitesse lente) ; console Yamaha.
- Nodal SDI, ST 2110 et NDI.

**Étape 2 : l'IA qui voit et qui anticipe**
- Visages, lèvres, locuteur actif audio-vidéo ; prédiction de la prise de parole.
- Strata Graphics (noms, synthés) ; Strata Multiview (UMD).
- Montage refait après coup pour l'enregistré (FCPXML, XML Premiere, OTIO).

**Étape 3 : l'IA qui comprend et qui apprend**
- Transcription et modèle de langage locaux.
- Classement des plans appris sur les reprises en main.
- Plusieurs studios par serveur.

---

## 9. Risques identifiés

| Risque | Parade |
| --- | --- |
| L'IA coupe en retard (le signal ne peut pas être retardé) | prédiction de la prise de parole, PTZ prêtes à l'avance, plans de repli |
| Repisse entre micros | calibrage à la vérification, comparaison relative, confirmation par l'image |
| Wi-Fi instable | la tablette n'est qu'une télécommande, commandes idempotentes, Stream Deck de secours |
| Tablette sans identification | réseau dédié qui ne voit que Studio ; actions sensibles en appui long |
| Un appareil change d'état à l'insu de Studio | état voulu / réel, alertes |
| Confiance des équipes | mode ombre, raisons affichées, reprise par le simple pupitre |
| Mixage qui « pompe » ou qui tarde | l'automix rapide reste dans la console ; l'IA ne fait que les réglages lents |

---

## 10. Maquettes

https://claude.ai/artifact/U6a7FSWuRyNGfwNzkCqmrG

- Page **Tablette · 5 pistes** : cinq directions de design pour la tablette, pendant l'émission (clair et calme, trois anneaux, tuiles, une chose à la fois, autour de la table).
- Page **Version 1** : le premier parcours complet de la tablette, et les pages Régie, Plateau et Scénario, remplacées depuis par la reprise sur le pupitre et le canvas de nœuds.

---

## 11. Questions ouvertes

1. **Destinations** : où vont le direct (encodeurs, plateformes) et les fichiers (disques de l'ATEM, HyperDeck, NAS, MAM) ? À définir.
2. **ST 2110** : quels appareils Studio doit-il connecter par IS-05 (passerelles, encodeurs, multiviewer) ?
3. **Rendre la main à l'IA** : après un délai sans geste humain (proposé), par un bouton de Buttons, ou les deux ?
4. **Automix** : les M32 et DM5 du studio ont-elles leur automix activable ?
