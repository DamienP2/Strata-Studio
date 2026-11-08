# Strata Studio : fonctionnalités (v6)

> Ce document est la référence du périmètre. Il prend le pas sur `PROPOSITION.md` partout où les deux diffèrent.
> Maquettes : https://claude.ai/artifact/U6a7FSWuRyNGfwNzkCqmrG (page « Prototypes interactifs »).

---

## 1. Fonctionnalités

| Domaine | Ce que fait Studio |
| --- | --- |
| **Réalisation** | IA locale qui commute un **ATEM** ou un **vMix** ; **débrayage manuel** possible à tout moment |
| **Caméras** | **Sony FR7 ou BRC-AM7** (mêmes protocoles : VISCA over IP, CGI) |
| **Audio** | micros reçus en **Dante** ou **AES67 / ST 2110-30**, au travers d'une console **Yamaha DM5** ou **Midas M32** |
| **Enregistrement** | **sur le serveur**, jusqu'à **6 canaux vidéo** : 4 caméras en ISO, le PGM et le CLEAN ; **un fichier audio par micro** |
| **Codecs** | **H.264 ou H.265, 4:2:2** |
| **Résolutions** | **1080p50** ou **2160p50** |
| **Formats** | **16:9** et/ou **9:16** |
| **Diffusion** | **Plateforme GO** (une parmi GO1 à GO4), **YouTube**, **TikTok** |
| **Flux extérieurs** | **Call** (plusieurs à la fois), **agence de presse**, **duplex Aviwest** |
| **Écran du studio** | un écran 16:9 sur le plateau : recommandations en vidéo, retour caméra pendant les tests, état du tournage |
| **Contrôle** | une **tablette**, ou un **Stream Deck avec un écran** : le même parcours, étape par étape (§ 3) |

---

## 2. Ce que ces choix impliquent

### 2.1 Une réalisation naturelle

L'IA ne se contente pas de suivre celui qui parle. Elle réalise comme un humain :

- **un plan large pour respirer**, régulièrement, et à chaque relance ;
- **des plans d'écoute** : celui qui écoute, qui réagit, qui va répondre ;
- **des mouvements très lents** sur les gros plans à l'antenne : un zoom avant à peine perceptible pour donner de la vie ;
- **le débrayage manuel** : une coupe faite au pupitre de l'ATEM ou dans vMix suspend l'IA ; la main lui revient par un bouton, ou après un temps sans geste.

Changement par rapport au squelette actuel : la règle « une caméra à l'antenne ne bouge jamais » devient « une caméra à l'antenne ne fait que des **mouvements lents et continus** (zoom de respiration) ; jamais de recadrage brusque ».

### 2.2 Studio entre dans le chemin de l'image

Jusqu'ici Studio ne faisait que piloter. **L'enregistrement et la diffusion sur le serveur** changent cela : Studio **reçoit**, **encode** et **envoie** des images.

```
 4 caméras ──┬──► mélangeur (ATEM / vMix) ──► PGM ──┬──► Studio : enregistrement (6 canaux)
             │                              └► CLEAN ┘            diffusion (GO, YouTube, TikTok)
             └──────────────────────────────────────────► Studio : ISO des 4 caméras, image pour l'IA
 micros ──► console (DM5 / M32) ──► Dante / AES67 ──► Studio : un fichier par micro, analyse de l'IA
 flux extérieurs (Call, agence, Aviwest) ──► mélangeur (et Studio pour les tests)
```

- **Entrée des images** : SDI par cartes DeckLink, NDI ou ST 2110 selon le studio ; à fixer (§ 5).
- **Encodage** : 6 canaux 4:2:2 en temps réel, plus les flux de diffusion. En 2160p50, c'est lourd. Les cartes NVIDIA de génération Blackwell encodent le H.264 et le H.265 en 4:2:2 en matériel ; la même carte sert l'IA. **Le nombre de canaux tenus en 2160p50 sera mesuré sur la machine retenue**, c'est le premier risque du projet.
- **Stockage** : l'enregistrement se fait sur les disques du serveur, puis Studio **copie** vers le stockage final choisi à l'étape 5.
- **Base existante** : Strata Graphics sait déjà recevoir (SDI DeckLink, NDI, SRT), encoder et diffuser (SRT, RTMP, HLS par ffmpeg, encodage matériel).

### 2.3 Le 9:16

- Le 9:16 est **découpé dans l'image 16:9**. En 2160p50, la découpe garde une bonne définition (1215 × 2160 avant mise à l'échelle) ; en 1080p50 elle est plus pauvre (608 × 1080).
- Quand le 9:16 est actif, l'IA **cadre plus large et plus centré** sur les gros plans, pour que la personne tienne dans la zone 9:16.
- **TikTok** reçoit le 9:16 ; YouTube et GO reçoivent le 16:9.

### 2.4 Ce qui est piloté par Buttons

L'**allumage** au début et l'**extinction** à la fin du studio (lumière par défaut, caméras) passent par Bitfocus Buttons, comme le reste des automations, sauf la mise sous tension des caméras qui peut aussi se faire en VISCA.

---

## 3. Le parcours de la tablette

La tablette ne fait que du **contrôle**, sans identification. Elle avance **une question à la fois** ; la colonne de gauche résume les réponses.

| # | Écran | Ce que fait la personne | Ce que fait Studio |
| --- | --- | --- | --- |
| 1 | **Accueil** | voit le logo, touche **« Commencer un tournage »** | allume la lumière par défaut et les caméras |
| 2 | **Vérification du studio** | regarde | vérifie lumière, caméras, mélangeur, console, audio Dante / AES67, espace disque, réseau de diffusion |
| 3 | **Nom du programme** | écrit le nom | nomme les fichiers |
| 4 | **Enregistrement** | choisit en **boutons à choix multiples** : PGM, CLEAN, caméras (ISO), micros ; « Tout » en raccourci | affiche le **temps d'enregistrement restant** selon les choix |
| 5 | **Stockage** | choisit où copier les fichiers après le tournage | vérifie la place libre |
| 6 | **Résolution** | Full HD (1080p50) ou 4K (2160p50) | recalcule le temps restant |
| 7 | **Format** | 16:9, 9:16, ou les deux | adapte les focales ; gros plans plus centrés si 9:16 |
| 8 | **Diffusion** | Plateforme GO (une parmi GO1 à GO4, **grisée si déjà utilisée**), YouTube, TikTok ; en boutons | prépare les flux |
| 9 | **Personnes** | nombre et noms | prépare un micro par personne |
| 10 | **Flux extérieurs** | Call (combien en même temps), agence de presse, duplex Aviwest | prépare les entrées du mélangeur |
| 11 | **Placement** | place chaque personne autour de la table | associe places, micros et caméras |
| 12 | **Tests** | une personne (puis une source extérieure) après l'autre, image et son | l'**écran du studio** montre la bonne caméra, la zone 9:16 (ou le 9:16 seul), le vumètre et la consigne en surimpression |
| 13 | **Récapitulatif** | relit, puis **lance l'enregistrement** (appui long) | dernières vérifications ; pendant le tournage, seulement la surveillance et **Terminer** (pas de pause, pas de touche toux) |
| 14 | **Fin de tournage** | lit le récapitulatif | arrête l'enregistrement, copie vers le stockage, éteint les caméras et la lumière |

### 3.1 Le même parcours sur un Stream Deck et un écran

En plus de la tablette, le studio peut se piloter avec **un Stream Deck posé à côté d'un écran**. C'est le même parcours, les mêmes étapes, la même logique : seule la façon de répondre change.

- **L'écran** affiche la question en cours, l'aide animée et le résumé des réponses : c'est la page de la tablette, en grand et sans toucher.
- **Les touches du Stream Deck** portent les réponses de l'étape en cours, avec leur texte et leur icône : *1080p* / *4K*, *16:9* / *9:16*, *GO1* à *GO4* (grisées si déjà prises), *YouTube*, *TikTok*… Une touche allumée est une réponse choisie. Deux touches fixes : **Retour** et **Valider**.
- **Les molettes** (Stream Deck +) règlent les nombres : personnes, Call simultanés.
- **Pendant le tournage**, les touches deviennent des voyants : enregistrement, chaque diffusion, son, IA ou manuel, et **Terminer** (appui long).
- **Ce qui s'écrit** (nom du programme, noms des personnes) se choisit dans des listes : programmes récents, annuaire des personnes déjà venues. Un nom nouveau se tape sur la tablette, ou sur un clavier branché au poste de l'écran.

Le Stream Deck passe par **Bitfocus Buttons**, qui gère déjà les surfaces : Studio lui donne, à chaque étape, les touches à afficher (texte, icône, état) et reçoit les appuis. **Le parcours ne vit qu'à un seul endroit, dans Studio** : la tablette, l'écran et le Stream Deck en sont des vues, et peuvent servir en même temps (une réponse donnée sur l'un apparaît aussitôt sur les autres).

## 4. L'écran du studio

Un écran 16:9 sur le plateau, piloté par Studio :

- **avant le tournage** : des **recommandations en vidéo** (se placer, parler au micro, regarder la caméra) ;
- **pendant les tests** : le **retour de la caméra** de la personne testée, avec la **zone 9:16** en surimpression (ou le 9:16 seul, au centre), le **vumètre** de son micro et la **consigne** (« Karim, dites votre nom et votre fonction ») ;
- **pendant le tournage** : l'état (enregistrement, direct) et le temps écoulé.

---

## 5. Questions ouvertes

1. **Étape 15** : la liste s'arrête au numéro 15, vide. Y a-t-il une étape de plus ?
2. **Entrée des images pour l'enregistrement** : SDI (cartes DeckLink), NDI ou ST 2110 ? Les ISO en NDI|HX seraient de moindre qualité qu'en SDI.
3. **PGM et CLEAN** : quelles sorties de l'ATEM ou de vMix les portent ?
4. **Conteneur des fichiers** : MOV, MP4 ou MXF ?
5. **Plateforme GO** : comment Studio sait-il qu'une GO est déjà utilisée, et comment s'y connecte-t-il (SRT, RTMP, API) ?
6. **Stockages finaux** : lesquels (NAS, MAM, autre) et par quel protocole ?
7. **Call** : quel outil (vMix Call, autre) et combien au maximum ?
8. **9:16 enregistré** : faut-il aussi enregistrer le PGM 9:16, ou seulement le diffuser ?
9. **Gros plans en 9:16** : nous avons compris « plus centrés » (pour tenir dans la découpe 9:16). Est-ce bien cela ?
