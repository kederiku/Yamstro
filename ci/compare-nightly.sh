#!/usr/bin/env bash
# Compare les taux de victoire de deux nuits, cellule par cellule.
#
# Usage : ci/compare-nightly.sh <répertoire de la veille> <répertoire de la nuit>
#
# **Il signale ; il ne fait jamais échouer.** Une dérive d'équilibrage est une
# information à lire le matin, pas une interruption : un job nocturne rouge que
# personne ne peut rejouer se désactive au bout de trois nuits, et l'archivage
# des artefacts disparaît avec lui. Seule la rupture d'empreinte entre les deux
# architectures fait échouer le job, et elle se traite dans le workflow.
#
# **La comparaison est entière, du fichier jusqu'au verdict.** Le rapport rend
# ses pourcentages à deux décimales exactes — `25,37 %` —, si bien que retirer
# la virgule et le signe redonne les dix-millièmes eux-mêmes, sans une seule
# division. Un flottant ferait diverger le seuil d'une plateforme à l'autre, sur
# la seule grandeur que ce job existe pour surveiller.
set -uo pipefail

VEILLE=${1:?répertoire de la veille}
NUIT=${2:?répertoire de la nuit}
SEUIL=300   # trois points, en dix-millièmes

# **La première nuit n'a rien à comparer, et le dit.** Un job qui échoue à sa
# première exécution est désactivé avant d'avoir servi une seule fois.
if [ ! -d "${VEILLE}" ] || [ -z "$(ls -A "${VEILLE}" 2>/dev/null)" ]; then
  printf 'aucune référence de la veille : rien à comparer.\n'
  exit 0
fi

# Les taux d'un rapport, un par ligne : « <libellé de cellule>|<dix-millièmes> ».
# Le libellé sert de clé : il porte le gobelet et la mise.
#
# Il est **normalisé** : l'alignement du rapport est un choix
# d'affichage, et deux nuits dont une colonne se serait élargie n'auraient plus
# aucune cellule en commun. Les espaces se réduisent à un.
#
# Et la séparation du libellé et du nombre exige **deux espaces au moins** :
# un seul rendrait le motif glouton, qui avalerait le premier chiffre du nombre
# dans la clé — mesuré, il rendait « mise 1     3 » et un taux de 310.
taux() {
  sed -n '/^-- taux de victoire par cellule --$/,/^$/p' "$1" \
    | sed -n 's/^  \(.*[^ ]\)  *\([0-9][0-9]*\),\([0-9][0-9]\) %.*$/\1|\2\3/p' \
    | tr -s ' '
}

signales=0
printf 'comparaison des taux de victoire, seuil %s dix-millièmes (trois points)\n' "${SEUIL}"

for actuel in "${NUIT}"/*.txt; do
  [ -e "${actuel}" ] || continue
  precedent="${VEILLE}/$(basename "${actuel}")"
  if [ ! -f "${precedent}" ]; then
    printf '  %s : absent de la veille\n' "$(basename "${actuel}")"
    continue
  fi

  while IFS='|' read -r cellule maintenant; do
    [ -n "${cellule}" ] || continue
    avant=$(taux "${precedent}" | sed -n "s/^${cellule}|//p")
    [ -n "${avant}" ] || continue
    if [ "${maintenant}" -ge "${avant}" ]; then
      ecart=$(( maintenant - avant ))
    else
      ecart=$(( avant - maintenant ))
    fi
    if [ "${ecart}" -gt "${SEUIL}" ]; then
      printf '  %s : %s -> %s (%s dix-millièmes)\n' \
        "${cellule}" "${avant}" "${maintenant}" "${ecart}"
      signales=$(( signales + 1 ))
    fi
  done <<EOF
$(taux "${actuel}")
EOF
done

if [ "${signales}" -eq 0 ]; then
  printf 'aucune cellule au-dessus du seuil.\n'
else
  printf '%s cellule(s) signalée(s).\n' "${signales}"
fi

exit 0
