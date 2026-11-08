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
| `strata-studio-devices` | les interfaces des appareils (mélangeur, nodal, PTZ, console, Bitfocus), leurs simulateurs, la console Midas M32 (OSC) |
| `strata-studio-server` | l'exécutable `strata-studio` : studio simulé, API d'état et de reprise en main |

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```
