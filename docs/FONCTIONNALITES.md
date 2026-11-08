# Strata Studio : fonctionnalités (v7)

> Ce document est la référence du périmètre. Il prend le pas sur `PROPOSITION.md` partout où les deux diffèrent.
> Maquettes : https://claude.ai/artifact/U6a7FSWuRyNGfwNzkCqmrG (page « Prototypes interactifs »).

**En trois gestes** : on allume le studio, on lance le tournage, on récupère les fichiers. Entre les deux, Studio réalise, enregistre, diffuse et range.

---

## 1. Fonctionnalités

| Domaine | Ce que fait Studio |
| --- | --- |
| **Réalisation** | IA locale qui commute un **ATEM** ou un **vMix** ; **débrayage manuel** possible à tout moment |
| **Caméras** | **Sony FR7 ou BRC-AM7** (mêmes protocoles : VISCA over IP, CGI) ; cadrage qui suit les personnes |
| **Audio** | micros reçus en **Dante** ou **AES67 / ST 2110-30**, au travers d'une console **Yamaha DM5** ou **Midas M32** |
| **Entrée des images** | **SDI** (cartes DeckLink), **NDI** et **ST 2110** |
| **Enregistrement** | **sur le serveur**, jusqu'à **6 canaux vidéo** : 4 caméras en ISO, le PGM et le CLEAN ; **un fichier audio par micro** ; **tout en 16:9** |
| **Codecs et fichiers** | **H.264 ou H.265, 4:2:2**, en **MP4** ou **MOV** |
| **Résolutions** | **1080p50** ou **2160p50** |
| **Formats** | **16:9**, et **9:16** en option : la réalisation est alors pensée pour la découpe verticale, et le serveur fabrique les versions 9:16 en plus |
| **Diffusion** | **Plateforme GO** (une parmi GO1 à GO4), **YouTube**, **TikTok** |
| **Flux extérieurs** | **Call** par **Quicklink StudioEdge** (Studiocall, Teams, Zoom ; plusieurs à la fois), **agence de presse**, **duplex Aviwest** |
| **Stockage final** | copie vers un partage **SMB** après le tournage |
| **Écran du studio** | un écran 16:9 sur le plateau : recommandations en vidéo, retour caméra pendant les tests, état du tournage |
| **Contrôle** | une **tablette**, ou un **Stream Deck avec un écran**, sur **tout le parcours** (§ 3) |

---

## 2. Ce que ces choix impliquent

### 2.1 Une réalisation naturelle

L'IA ne se contente pas de suivre celui qui parle. Elle réalise comme un humain :

- **un plan large pour respirer**, régulièrement, et à chaque relance ;
- **des plans d'écoute** : celui qui écoute, qui réagit, qui va répondre ;
- **des mouvements très lents** sur les gros plans à l'antenne : un zoom avant à peine perceptible pour donner de la vie ;
- **un cadrage qui suit** : si un invité se penche ou recule, la caméra corrige doucement son cadre ;
- **le débrayage manuel** : une coupe faite au pupitre de l'ATEM ou dans vMix suspend l'IA ; la main lui revient par un bouton, ou après un temps sans geste.

Changement par rapport au squelette actuel : la règle « une caméra à l'antenne ne bouge jamais » devient « une caméra à l'antenne ne fait que des **mouvements lents et continus** (respiration, suivi) ; jamais de recadrage brusque ».

### 2.2 Studio entre dans le chemin de l'image

Jusqu'ici Studio ne faisait que piloter. **L'enregistrement et une partie de la diffusion** changent cela : Studio **reçoit**, **encode** et **envoie** des images.

```
 4 caméras ──┬──► mélangeur (ATEM / vMix) ──► PGM ───┬──► Studio : enregistrement (6 canaux 16:9)
             │                              └► CLEAN ─┘            YouTube, TikTok (9:16 découpé en direct)
             │                                 PGM ──► Videohub ──► encodeur GO choisi (GO1 à GO4)
             └──────────────────────────────────────────► Studio : ISO des 4 caméras, image pour l'IA
 micros ──► console (DM5 / M32) ──► Dante / AES67 ──► Studio : un fichier par micro, analyse de l'IA
 Quicklink StudioEdge (Call), agence, Aviwest ──► mélangeur (et Studio pour les tests)
```

- **Entrée des images** : SDI (DeckLink), NDI et ST 2110 ; chaque source se déclare dans son format.
- **PGM et CLEAN** : deux sorties du mélangeur arrivent au serveur. Sur un ATEM, le PGM sort sur sa sortie programme et le CLEAN sur une sortie auxiliaire réglée en *clean feed* (le programme sans les incrustations). Sur vMix, deux sorties (SDI ou NDI) : le programme, et le programme sans les overlays. **Quelle sortie porte quoi se règle dans les paramètres, côté administration**, pour l'ATEM comme pour vMix.
- **Encodage** : 6 canaux 4:2:2 en temps réel, plus YouTube et TikTok. En 2160p50, c'est lourd. Les cartes NVIDIA de génération Blackwell encodent le H.264 et le H.265 en 4:2:2 en matériel ; la même carte sert l'IA. **Le nombre de canaux tenus en 2160p50 sera mesuré sur la machine retenue**, c'est le premier risque du projet.
- **Plateforme GO** : chaque GO est un **encodeur Blackmagic Web Encoder 4K**. Studio lit par son API s'il diffuse déjà : un GO en service est **grisé** sur la tablette. Pour le GO choisi, Studio route le PGM vers son entrée par le Videohub, puis **lance la diffusion au lancement du tournage et l'arrête à la fin**, par la même API.
- **YouTube et TikTok** : encodés par le serveur ; TikTok reçoit le PGM découpé en 9:16, en direct.
- **Stockage** : l'enregistrement se fait sur les disques du serveur, puis Studio **copie** vers le partage SMB choisi, rangé par programme et par date.
- **Base existante** : Strata Graphics sait déjà recevoir (SDI DeckLink, NDI, SRT), encoder et diffuser (SRT, RTMP, HLS par ffmpeg, encodage matériel).

### 2.3 Le 9:16

- **Tout est enregistré en 16:9.** Si le 9:16 est choisi, la réalisation est **pensée pour la découpe** : gros plans plus larges et **plus centrés**, pour que la personne tienne dans la zone 9:16.
- **Après le tournage**, le serveur **fabrique les versions 9:16** du PGM, du CLEAN et des ISO caméras, en plus des versions 16:9, avant de les copier. Les faire après le tournage évite de doubler la charge d'encodage pendant l'enregistrement.
- En 2160p50, la découpe garde une bonne définition (1215 × 2160) ; en 1080p50 elle est plus pauvre (608 × 1080). La tablette le dit quand on choisit 9:16 en Full HD.

### 2.4 Les flux extérieurs

- **Call** : Quicklink StudioEdge reçoit les appels **Studiocall, Teams ou Zoom** et les sort vers le mélangeur. Studio prépare autant d'entrées que d'appels simultanés choisis. Pour le pilotage des appels, la référence est la documentation StudioCall Live de Quicklink (support.quicklink.tv, catégorie « QuickLink StudioCall Live ») : Studio passe par son interface de contrôle si elle couvre le besoin, sinon par Buttons. Ce point reste à confirmer, la documentation n'ayant pas encore pu être lue (accès protégé).
- **Agence de presse** et **duplex Aviwest** : une entrée du mélangeur chacun, testée comme une personne (image et son) à l'étape 12.

### 2.5 Ce qui est piloté par Buttons

L'**allumage** au début et l'**extinction** à la fin (lumière par défaut, caméras) passent par Bitfocus Buttons, comme le reste des automations ; la mise sous tension des caméras peut aussi se faire en VISCA. Le **Stream Deck** passe aussi par Buttons (§ 3.2).

---

## 3. Le parcours

Le contrôle se fait **une question à la fois**, sans identification. Une colonne résume les réponses.

| # | Écran | Ce que fait la personne | Ce que fait Studio |
| --- | --- | --- | --- |
| 1 | **Accueil** | voit le logo, touche **« Commencer un tournage »** | allume la lumière par défaut et les caméras |
| 2 | **Vérification du studio** | regarde | vérifie lumière, caméras, mélangeur, console, audio Dante / AES67, Videohub, encodeurs GO, espace disque |
| 3 | **Nom du programme** | écrit le nom, ou choisit un programme déjà fait | nomme les fichiers ; **reprend les réglages du programme** (§ 3.1) |
| 4 | **Enregistrement** | choisit en **boutons à choix multiples** : PGM, CLEAN, caméras (ISO), micros ; « Tout » en raccourci | affiche le **temps d'enregistrement restant** selon les choix |
| 5 | **Stockage** | choisit le partage SMB où copier les fichiers après le tournage | vérifie la place libre |
| 6 | **Résolution** | Full HD (1080p50) ou 4K (2160p50) | recalcule le temps restant |
| 7 | **Format** | 16:9 seul, ou 16:9 et 9:16 | cadrages pensés pour le 9:16 ; versions 9:16 fabriquées après |
| 8 | **Diffusion** | Plateforme GO (une parmi GO1 à GO4, **grisée si déjà utilisée**), YouTube, TikTok ; en boutons, ou aucune | prépare les flux et les routes |
| 9 | **Personnes** | nombre et noms | prépare un micro par personne |
| 10 | **Flux extérieurs** | Call (combien en même temps), agence de presse, duplex Aviwest | prépare les entrées du mélangeur |
| 11 | **Placement** | place chaque personne autour de la table | associe places, micros et caméras |
| 12 | **Tests** | une personne, puis chaque source extérieure, l'une après l'autre : image et son | l'**écran du studio** montre la bonne caméra, la zone 9:16 (ou le 9:16 seul), le vumètre et la consigne en surimpression |
| 13 | **Récapitulatif** | relit, puis **lance l'enregistrement** (appui long) | dernières vérifications ; pendant le tournage, seulement la surveillance et **Terminer** (pas de pause, pas de touche toux) |
| 14 | **Fin de tournage** | lit le récapitulatif | arrête l'enregistrement, fabrique les versions 9:16, copie vers le stockage, éteint les caméras et la lumière |

### 3.1 Les profils de programme

Un programme déjà tourné dans le studio garde **son profil** : enregistrement, stockage, résolution, formats, diffusion, personnes habituelles, flux extérieurs, placement, ambiance lumineuse.

À l'étape 3, choisir un programme existant **pré-remplit toutes les étapes suivantes**. On peut les relire une à une, ou passer directement aux tests (« Comme la dernière fois »). Le profil est mis à jour à la fin de chaque tournage.

### 3.2 Le même parcours sur un Stream Deck et un écran

En plus de la tablette, le studio se pilote avec **un Stream Deck posé à côté d'un écran**, sur **tout le parcours**. Mêmes étapes, même logique : seule la façon de répondre change.

- **L'écran** affiche la question en cours, l'aide animée et le résumé des réponses : c'est la page de la tablette, en grand et sans toucher.
- **Les touches** portent les réponses de l'étape en cours, avec leur texte et leur icône : *1080p* / *4K*, *16:9* / *9:16*, *GO1* à *GO4* (grisées si déjà pris), *YouTube*, *TikTok*… Une touche allumée est une réponse choisie. Deux touches fixes : **Retour** et **Valider**.
- **Les molettes** (Stream Deck +) règlent les nombres : personnes, Call simultanés.
- **Pendant le tournage**, les touches deviennent des voyants : enregistrement, chaque diffusion, son, IA ou manuel, et **Terminer** (appui long).
- **Ce qui s'écrit** (nom du programme, noms des personnes) se choisit dans des listes : programmes du studio, personnes déjà venues. Un nom nouveau se tape sur la tablette, ou sur un clavier branché au poste de l'écran.

Le Stream Deck passe par **Bitfocus Buttons**, qui gère déjà les surfaces : Studio lui donne, à chaque étape, les touches à afficher (texte, icône, état) et reçoit les appuis. **Le parcours ne vit qu'à un seul endroit, dans Studio** : la tablette, l'écran et le Stream Deck en sont des vues, et peuvent servir en même temps.

---

## 4. L'écran du studio

Un écran 16:9 sur le plateau, piloté par Studio :

- **avant le tournage** : des **recommandations en vidéo** (se placer, parler au micro, regarder la caméra) ;
- **pendant les tests** : le **retour de la caméra** de la personne testée, avec la **zone 9:16** en surimpression (ou le 9:16 seul, au centre), le **vumètre** de son micro et la **consigne** (« Karim, dites votre nom et votre fonction ») ;
- **pendant le tournage** : toujours visibles, l'**enregistrement**, chaque **diffusion**, les **niveaux audio** et le temps écoulé.

---

## 5. Propositions à valider

1. **Habillage depuis le Stream Deck pendant le tournage** : lancer l'intro, le nom d'un intervenant, un habillage (par Strata Graphics), sans toucher à la réalisation.
2. **Dispositions** : plein écran, côte à côte, écran d'attente, en une touche.
3. **Ambiance lumineuse** : l'ambiance fait partie du profil de programme et se rappelle avec lui.
4. **Copie sur un disque externe** (USB, SSD) en plus du partage SMB.

## 6. Questions ouvertes

1. **StudioEdge** : que permet son interface de contrôle (lancer ou accepter un appel, l'envoyer sur une sortie, lire son état et ses niveaux) ? La documentation de Quicklink est derrière une protection anti-robot : un extrait, ou une capture de la page API, suffira.
