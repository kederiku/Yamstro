#!/usr/bin/env bash
# Compare la mesure à la référence versionnée.
#
# Échoue si la mesure dépasse la référence de plus de 5 %, ou la cible absolue
# de 25 Mo. Le premier seuil veut qu'un dépassement franchi sans intention se
# discute en revue plutôt qu'il ne se découvre trois étapes plus tard ; le
# second est la contrainte du projet, et il ne se négocie pas.
#
# **La référence ne se met jamais à jour toute seule.** Une référence qui se
# réécrit depuis la CI ne garde rien : elle enregistre la dérive au lieu de la
# signaler. Elle se change par une modification explicite de
# `ci/wasm-size-baseline.txt`, revue comme le reste.
#
# **Deux grandeurs, deux seuils (TASK-93).** Le `.wasm` optimisé seul se
# compare à la référence, à +5 % près ; le `.wasm` optimisé **plus `assets/`**,
# ce qui est réellement livré, se compare à la cible absolue de 25 Mo. Les
# shaders pèsent quelques kilo-octets ; les images qu'ils échantillonneront un
# jour, non. Les assets se pèsent par `cat | wc -c`, portable : `du -sb` est
# une option GNU que darwin rejette, et un `xargs wc` sans fichier lirait
# l'entrée standard.
set -euo pipefail

REFERENCE_FICHIER=ci/wasm-size-baseline.txt
CIBLE_ABSOLUE=26214400   # 25 Mo
MARGE_POUR_CENT=5

mesure=$(ci/measure-wasm.sh "${1:-dist}")
reference=$(tr -d '[:space:]' < "${REFERENCE_FICHIER}")
assets=$(find assets -type f -exec cat {} + | wc -c | tr -d ' ')
total=$(( mesure + assets ))

plafond=$(( reference * (100 + MARGE_POUR_CENT) / 100 ))
ecart=$(( mesure - reference ))
pour_mille=$(( ecart * 1000 / reference ))

printf 'mesure    : %s octets\n' "${mesure}"
printf 'référence : %s octets\n' "${reference}"
printf 'écart     : %s octets (%s pour mille)\n' "${ecart}" "${pour_mille}"
printf 'plafond   : %s octets (référence +%s %%)\n' "${plafond}" "${MARGE_POUR_CENT}"
printf 'assets    : %s octets\n' "${assets}"
printf 'total     : %s octets (.wasm optimisé + assets)\n' "${total}"
printf 'cible     : %s octets (25 Mo)\n' "${CIBLE_ABSOLUE}"

statut=0
if [ "${mesure}" -gt "${plafond}" ]; then
  printf 'ECHEC : la mesure dépasse la référence de plus de %s %%.\n' "${MARGE_POUR_CENT}"
  statut=1
fi
if [ "${total}" -gt "${CIBLE_ABSOLUE}" ]; then
  printf 'ECHEC : le total, assets compris, dépasse la cible absolue de 25 Mo.\n'
  statut=1
fi

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    printf '## Taille du binaire WebAssembly\n\n'
    printf '| | octets |\n| :-- | --: |\n'
    printf '| mesure | %s |\n' "${mesure}"
    printf '| référence | %s |\n' "${reference}"
    printf '| écart | %s |\n' "${ecart}"
    printf '| assets | %s |\n' "${assets}"
    printf '| total, assets compris | %s |\n' "${total}"
    printf '| cible absolue | %s |\n' "${CIBLE_ABSOLUE}"
  } >> "${GITHUB_STEP_SUMMARY}"
fi

exit "${statut}"
