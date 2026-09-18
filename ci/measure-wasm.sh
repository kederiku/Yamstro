#!/usr/bin/env bash
# Mesure de la taille du binaire WebAssembly optimisé.
#
# Source unique de la procédure : la CI et la machine de développement lancent
# ce script, jamais une suite de commandes recopiée. Une procédure recopiée
# diverge, et une référence mesurée autrement qu'elle n'est vérifiée ne garde
# rien.
#
# **La grandeur mesurée est la sortie de `wasm-opt -Oz`**, jamais l'artefact
# brut de `cargo build` : l'écart entre les deux se compte en mégaoctets.
#
# `wc -c` et non `stat -c %s` : cette dernière est une option GNU, que darwin
# rejette. La procédure doit tourner sur la machine de développement, sinon
# personne ne la relance avant de pousser.
#
# **Deux modes, pour que le coût du son se mesure** (TASK-108). Sans second
# argument, le jeu entier, son compris : c'est la grandeur de référence. Avec
# `sans-audio`, le même jeu sans la feature `audio` de la cible ; la différence
# des deux sorties est ce que le son coûte, et la porte de taille en tient un
# plancher. Usage : measure-wasm.sh [dossier de sortie] [sans-audio]
set -euo pipefail

CIBLE=wasm_size
SORTIE=${1:-dist}
case "${2:-}" in
  "")         FEATURES=(--features measure) ;;
  sans-audio) FEATURES=(--no-default-features --features measure) ;;
  *)          printf 'mode inconnu : %s\n' "$2" >&2; exit 2 ;;
esac
BRUT=target/wasm32-unknown-unknown/release/${CIBLE}.wasm

cargo build --release --target wasm32-unknown-unknown \
  -p wasm_size "${FEATURES[@]}" --bin "${CIBLE}"

rm -rf "${SORTIE}"
wasm-bindgen --target web --out-dir "${SORTIE}" "${BRUT}"
# **`wasm-opt -Oz` seul échoue**, et le corpus ne le dit pas : binaryen valide
# le module d'entrée contre un jeu de fonctionnalités par défaut plus étroit
# que ce que rustc émet pour cette cible. Sans ces drapeaux, l'outil s'arrête
# sur « error validating input », d'abord sur `memory.copy`, puis sur
# `i32.trunc_sat_f32_u`. Ce sont les fonctionnalités du profil WebAssembly que
# rustc active par défaut ; les activer ne change rien au module produit, cela
# autorise seulement le validateur à le lire.
FONCTIONNALITES=(
  --enable-bulk-memory
  --enable-bulk-memory-opt
  --enable-nontrapping-float-to-int
  --enable-sign-ext
  --enable-mutable-globals
  --enable-multivalue
  --enable-reference-types
  --enable-extended-const
)

# `-Oz` et non `-O3` : l'objectif est la taille, pas la vitesse d'un jeu de dés.
wasm-opt "${FONCTIONNALITES[@]}" -Oz \
  -o "${SORTIE}/${CIBLE}_bg.opt.wasm" "${SORTIE}/${CIBLE}_bg.wasm"

wc -c < "${SORTIE}/${CIBLE}_bg.opt.wasm" | tr -d ' '
