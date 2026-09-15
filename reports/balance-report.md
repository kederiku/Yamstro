# Premier rapport d'équilibrage — Yamstro

> **Un harnais livré sans ce rapport n'a rien démontré.**

Ce document est le premier chiffre du projet. Aucune valeur du corpus n'avait jamais été mesurée :
ni la courbe, ni le budget de puissance par rareté, ni les taux de rareté, ni le plafond d'intérêts,
ni les prix. Ce qui suit les mesure.

**Il remet en cause, il ne corrige pas.** Les corrections appartiennent au propriétaire de l'Étape 6
pour la courbe et l'économie, à l'Étape 9 pour le catalogue, à l'Étape 10 pour les mises.

| | |
| :-- | :-- |
| **Date** | 15 septembre 2026 |
| **Commit** | `b82daa0` |
| **`seed_base`** | 1 |
| **Commande** (passe de référence) | `cargo run -p sim_harness --release -- --runs 10000 --seed-base 1 --policy grid-aware --shop-policy budget --out reports/balance.csv --report` |
| **Commandes de comparaison** | la même, `--policy greedy` puis `--policy random`, sans `--out` |
| **Machine** | Apple M4 Pro, Darwin arm64 |
| **Threads** | 14 |
| **Débit** | **150 000 runs en 1,21 s de temps mural**, 13,02 s de temps CPU |

La campagne couvre la matrice complète — cinq gobelets par trois mises — à 10 000 runs par cellule,
soit **150 000 runs** par passe. Le tableau joint est celui de la passe de référence ; les deux
autres passes sont jointes sous forme de sortie console.

Le critère de débit de l'étape demande dix mille runs en moins de soixante secondes sur huit cœurs.
Mesuré : **quinze fois ce volume en une seconde et deux dixièmes**. Le débit n'est pas la contrainte
de cet instrument, et il ne le sera pas avant longtemps.

## 1. Le témoin

**Il se lit avant tout le reste.** Si la sonde gloutonne ne domine pas nettement le témoin aléatoire,
l'instrument est faux avant le jeu et aucun chiffre de ce rapport ne vaut.

| nombre de runs atteignant… | aléatoire | gloutonne |
| :-- | --: | --: |
| l'ante 2 | 254 | **66 891** |
| l'ante 3 | 33 | **34 431** |
| l'ante 4 | 3 | **10 330** |
| l'ante 5 | 0 | **1 550** |

Le rapport médian score sur cible à l'ante 1 vaut **64,00 %** pour le témoin aléatoire contre
**96,80 %** pour la sonde gloutonne. L'écart est d'un ordre de grandeur sur chaque palier de
profondeur.

**Le témoin tient. Les chiffres qui suivent sont interprétables.**

## 2. Les sept questions

| question | mesure | lecture |
| :-- | :-- | :-- |
| La courbe est-elle franchissable à l'Ante 8 ? | rapport médian score sur cible par ante : **94,66 %** · 100,00 % · 87,32 % · 81,10 % · 77,51 % · 75,96 % · 71,54 % · non atteint | **le mur est à l'ante 1.** Sur 149 944 runs, **un seul** atteint l'ante 7, et **aucun** l'ante 8 |
| Le budget de puissance par rareté est-il respecté ? | contributions médianes par palier : **352** · **559** · **194** | le rapport observé est **1 : 1,59 : 0,55** — le palier rare contribue **moins** que le commun. Voir § 5 : la médiane est biaisée |
| Quelles reliques sont hors budget ? | taux de conservation : **8 reliques sur 12** hors de la bande 10 – 80 % | trois reliques concentrent **170 256 achats** sur 258 095 ; quatre en totalisent **6 146** |
| Les gobelets ont-ils des taux de victoire resserrés ? | les **15** cellules valent **0,00 %** | **non mesurable** : l'écart entre gobelets n'est pas défini quand aucun ne produit de victoire |
| Les 6 mises forment-elles une progression régulière ? | trois mises couvertes, toutes à 0,00 % | **non mesurable avant l'Étape 10** : deux effets seulement sont implémentés, et quatre colonnes seraient jumelles |
| L'économie est-elle un levier ou un bruit de fond ? | intérêts **187 444** sur **2 224 943** d'or gagné, soit **8,4 %** ; or gagné moyen par run **14,8** | **bruit de fond.** Le plafond d'intérêts vaut vingt-cinq pièces et la bourse moyenne d'un run entier n'en voit pas quinze : le plafond **ne décide rien** |
| L'ADR-001 crée-t-il une décision ? | voir ci-dessous | **oui, et elle est négative** |

**La première question mérite son paragraphe.** Le premier ante où la médiane du rapport score sur
cible passe sous 1,00 est le **mur réel** de la run — pas l'ante de défaite médian, qui mêle la
courbe, l'économie, les reliques et la politique. C'est le seul chiffre qui isole la courbe
elle-même. Il vaut **1**. La première Petite Mise du jeu demande déjà, en médiane, plus que ce que la
meilleure main d'un run produit. La montée à 100,00 % à l'ante 2 n'est pas une embellie : c'est le
biais de survie des 48 985 runs qui y sont parvenus.

**La dernière est le résultat le plus important de l'étape, et il faut d'abord dire ce qu'il n'est
pas.** Le critère demande l'écart entre la sonde attentive à la grille et la sonde gloutonne **en
points de taux de victoire**. Cet écart vaut **0 point**. Ce zéro ne mesure rien : les deux sondes
valent 0,00 % parce que **personne ne gagne**, pas parce que les deux se valent. Lu tel quel, il
conclurait « écart nul, donc grille inerte » — et ce serait plus doux que la vérité.

Sur les grandeurs qui discriminent, l'écart existe, il est net, et il est **du mauvais côté** :

| nombre de runs atteignant… | attentive à la grille | gloutonne | écart |
| :-- | --: | --: | --: |
| l'ante 2 | 48 985 | 66 891 | **+36 %** pour la gloutonne |
| l'ante 3 | 24 512 | 34 431 | **+40 %** |
| l'ante 4 | 7 414 | 10 330 | **+39 %** |
| l'ante 5 | 1 084 | 1 550 | **+42 %** |

**La sonde qui respecte la grille consommable va moins loin que celle qui l'ignore, de quarante pour
cent, à chaque palier.** Le pilier de gameplay du projet ne crée pas une décision neutre : il crée
une décision **perdante**. Un joueur qui raisonne sur la grille est puni par rapport à un joueur qui
prend le plus gros score à chaque main.

Cela ne dit pas que l'ADR-001 est faux. Cela dit que, **au calibrage actuel**, la grille ne paie
pas — et que la cause la plus probable est le mur de l'ante 1 : quand la première manche est déjà
hors de portée, préserver une figure pour plus tard revient à renoncer à des points qu'on ne
récupérera jamais. La question se rejoue le jour où la courbe est franchissable, et **elle doit se
rejouer avant que l'Étape 9 ne construise soixante reliques par-dessus**.

## 3. Les trois arbitrages

### (a) Les doublons d'identifiant dans un même étalage

**Mesure : 12 334 étalages sur 100 000 portent deux fois la même relique, soit 12,3 %.**

Un étalage sur huit gaspille l'une de ses deux cases de relique. Ce n'est pas un accident rare.

**Recommandation : exclure le doublon, sans retirage.** Le générateur tire les deux cartes par une
boucle de deux passes, chacune tirant d'abord un palier de rareté puis une définition dans le vivier
de ce palier. Il suffit, **quand les deux paliers coïncident**, de tirer la seconde définition dans
un intervalle amputé d'une unité et de décaler l'index au-delà de la première. Un seul tirage, aucune
boucle, **aucun décalage de flux** : deux graines identiques donnent toujours deux étalages
identiques.

C'est la condition que le renvoi posait — une recommandation qui exigerait un retirage ne serait pas
actionnable, puisqu'une boucle de retirage décale le flux dès qu'un doublon sort.

### (b) *La Cage* et son slot vide

Deux mesures, et elles concordent.

**L'inventaire vu au moment des Mises Boss compte 3,32 slots occupés sur 5** en moyenne, sur 90 442
mains observées. Le slot mis en cage étant tiré uniformément sur la plage de capacité, **le slot
tiré est vide dans 33,6 % des cas** : un tiers des Cages ne grise rien.

**Et le taux d'échec confirme l'absence de mordant**, sur 20 000 runs :

| boss | Mises Boss vues | franchies | taux d'échec |
| :-- | --: | --: | --: |
| *L'Oubli* | 2 056 | 620 | 70 % |
| *Le Borgne* | 2 020 | 590 | 71 % |
| *La Fissure* | 2 025 | 556 | 73 % |
| *L'Étau* | 1 970 | 507 | 75 % |
| ***La Cage*** | 2 110 | 520 | **76 %** |
| ***Le Silex*** | 2 037 | 63 | **97 %** |

*La Cage* est **indiscernable de *L'Étau*** à un point près, et se tient dans un peloton de six
points. Elle n'est pas inerte, mais elle n'est pas non plus un boss : elle ne se distingue d'aucune
autre contrainte.

**Recommandation : tirer le slot parmi les slots occupés**, le jour où une capacité variable ne
menace plus la reproductibilité — ou, à défaut, donner à *La Cage* un second effet. Et voir § 4 : ce
tableau a produit une découverte qu'on ne cherchait pas.

### (c) Le signal de calibrage n° 1 — la cible empilée

**L'arbitrage ne se tranche pas sur un taux de victoire, et il faut dire pourquoi.** *Le Mur* est une
fixture de mise à l'échelle et n'entre dans aucun catalogue de boss livré ; le tirage porte sur six
boss dont **aucun** ne porte de multiplicateur de cible. La campagne ne peut donc **jamais** produire
cet empilement : un taux demandé sur lui porterait sur **zéro run** et sortirait 0 %, un chiffre qui
se lirait comme une impasse confirmée alors qu'il ne mesure rien.

**Le chiffre qui tranche est la cible elle-même**, rendue par un appel pur à la fonction de cible,
pour l'empilement *Le Mur* + Gobelet de Fortune + Mise 3 :

| ante | cible empilée | Boss neutre, Fortune, Mise 3 | Boss standard, Mise 1 | rapport à la Boss standard |
| --: | --: | --: | --: | --: |
| 1 | 2 250 | 750 | 600 | **×3,75** |
| 2 | 4 140 | 1 380 | 960 | ×4,31 |
| 3 | 7 615 | 2 538 | 1 536 | ×4,96 |
| 4 | 14 008 | 4 669 | 2 458 | ×5,70 |
| 5 | 25 775 | 8 592 | 3 932 | ×6,56 |
| 6 | 47 422 | 15 807 | 6 292 | ×7,54 |
| 7 | 87 237 | 29 079 | 10 066 | ×8,67 |
| 8 | 160 477 | 53 492 | 16 106 | **×9,96** |

Le rapport annoncé à la rédaction de l'Étape 6 est **exactement retrouvé** : de ×3,75 à l'ante 1 à
**×9,96** à l'ante 8. La cause est structurelle — seul le facteur de mise compose avec l'ante.

**Et la franchissabilité se lit tout de suite.** La meilleure main d'un run vaut **204** en médiane,
1 012 au neuvième décile, 2 030 au quatre-vingt-dix-neuvième. La cible empilée de l'**ante 1** vaut
**2 250** : elle est déjà hors de portée du quatre-vingt-dix-neuvième centile d'une main, et une
manche se gagne en quatre mains au plus.

**L'ante de bascule est donc l'ante 1**, et c'est la conclusion qui compte : l'empilement n'a pas
besoin de l'ante 8 pour être injouable, il l'est dès la première manche. Le problème n'est pas la
composition des facteurs à l'ante 8 — c'est que la courbe de base est déjà infranchissable, et que
l'empilement ne fait que multiplier une cible déjà trop haute. **La correction se fait dans la
composition des facteurs, et elle vient après celle de la courbe** ; corriger l'empilement seul
laisserait le jeu exactement aussi injouable.

## 4. Les valeurs remises en cause

Six, chacune adossée au nombre qui la contredit.

| # | valeur | étape | le nombre qui la contredit | correction proposée |
| :-: | :-- | :-: | :-- | :-- |
| 1 | la courbe de difficulté — terme initial et croissance | 6 | **le mur est à l'ante 1** : rapport médian score sur cible **94,66 %** dès la première manche ; **aucun** run sur 149 944 n'atteint l'ante 8, **un seul** l'ante 7 | baisser le terme initial d'abord ; ne toucher à la croissance qu'ensuite, et seulement si le mur ne remonte pas assez haut |
| 2 | *Le Silex* | 6 | **97 %** d'échec contre 70 – 76 % pour les cinq autres boss, sur 2 037 rencontres | ramener sa contrainte dans le peloton ; c'est le seul boss dont l'échec est quasi certain |
| 3 | le plafond d'intérêts | 6 | les intérêts font **8,4 %** de l'or gagné ; l'or gagné moyen d'un run entier vaut **14,8** contre un plafond de vingt-cinq | soit abaisser le plafond pour qu'il morde, soit augmenter les revenus pour qu'il soit atteignable — en l'état c'est un faux choix |
| 4 | la bande de conservation 10 – 80 % | 5 | **8 reliques sur 12** hors bande ; trois d'entre elles conservées à plus de **83 %**, quatre à moins de **9 %** | la bande décrit un catalogue équilibré, pas celui-ci ; à réexaminer après correction des prix |
| 5 | les prix, et la table de rareté qui les alimente | 5 et 6 | trois reliques totalisent **170 256** achats sur 258 095 ; quatre autres en totalisent **6 146**. La sonde d'achat au budget prend les moins chères et ne voit jamais les autres | rapprocher les prix, ou rendre la sonde sensible à autre chose qu'au prix — sans quoi le catalogue n'est jamais échantillonné |
| 6 | *La Cage* | 6 | **33,6 %** de slots vides et **76 %** d'échec, indiscernable de *L'Étau* | tirer le slot parmi les slots occupés, ou lui donner un second effet |

**La deuxième n'était cherchée par personne.** Le tableau des taux d'échec par boss a été produit pour
trancher l'arbitrage sur *La Cage* ; il a montré *Le Silex* à vingt-quatre points au-dessus du
deuxième. C'est la démonstration la plus nette que l'instrument sert à quelque chose : il répond à
des questions qu'on n'a pas posées.

## 5. La conversion des multiplicateurs en unités de budget

Le glossaire donne deux paliers additifs et deux paliers multiplicatifs, et **aucune conversion entre
les deux**. Le ratio **1 : 2 : 3,75** que le document d'étape emploie la suppose ; il est repris ici
comme **hypothèse**, jamais comme cible.

**Cette campagne ne permet pas de proposer la conversion**, et il vaut mieux le dire que produire un
coefficient.

Deux raisons, chiffrées.

**La première est un biais de la mesure elle-même.** La médiane de contribution d'un palier est prise
sur les quatre reliques du palier, **y compris celles qui ne produisent pas de score** — la tirelire
et le dé fantôme produisent de l'or et des faces, et entrent dans la médiane comme des zéros. Les
trois médianes publiées — 352, 559, 194 — sont donc tirées vers le bas sur le palier commun et le
palier peu commun, et pas sur le palier rare, dont les quatre reliques marquent toutes. **Le rapport
1 : 1,59 : 0,55 compare des grandeurs qui ne mesurent pas la même chose.**

**La seconde est la dispersion interne, et elle est décisive.** À l'intérieur du seul palier rare,
les contributions moyennes vont de **107** à **4 570** — un facteur **43**, sur 1 495 à 1 601 achats
chacune. Une médiane de palier n'a aucun sens sur une distribution aussi étalée, et une conversion
bâtie dessus serait citée comme un résultat.

Contributions moyennes mesurées, par relique et par palier :

| palier | reliques qui marquent | contributions moyennes | achats |
| :-- | :-- | :-- | --: |
| Commune | Dé Fêlé, Pierre Polie, Maître des Triplés | 1 326 · 1 230 · 352 | 53 775 · 55 094 · 61 387 |
| Peu commune | Architecte du Full, Alignement Stellaire, Pyramide de Six | 814 · 1 265 · 559 | 5 076 · 5 062 · 4 957 |
| Rare | Pendule, Obsidienne Instable, Yams Divin, Miroir Double | 1 523 · **4 570** · **107** · 194 | 1 481 · 1 495 · 1 569 · 1 601 |

**Ce que la campagne établit, et qui est actionnable dès maintenant** : deux reliques rares sont hors
de toute fourchette raisonnable — l'Obsidienne Instable contribue **plus de trois fois** la meilleure
commune, le Yams Divin **moins du tiers** de la plus faible. Ces deux-là se revalorisent sans
attendre aucune conversion.

**Ce qu'il faut pour proposer la conversion** : un catalogue dont les paliers soient échantillonnés
comparablement, et une médiane de palier qui exclue les reliques hors du chemin de score. Les deux
sont des corrections, pas des mesures.

## 6. La règle de correction

**Hors des cibles, la valeur se corrige dans les définitions, jamais dans la formule de score.** La
formule reste le produit des mises par le multiplicateur, arrondi au centième près : il n'existe
aucune action qui multiplierait le score, aucune formule enfichable. Une correction qui toucherait la
formule réglerait un symptôme sur toutes les valeurs à la fois et rendrait tout l'équilibrage
antérieur incomparable — y compris les 150 000 runs de ce rapport.

**Pour la courbe, l'ordre est normatif : le terme initial d'abord — trois cents —,
la croissance par ante ensuite.** Le premier translate la difficulté ; la seconde change sa forme, donc la position du
mur, donc l'ante de défaite médian, donc les taux de conservation, donc les contributions par
rareté. Une correction de croissance invalide **toutes** les mesures antérieures ; une correction du
terme initial n'en invalide qu'une partie.

**Les deux nombres de la courbe sont écrits ici en toutes lettres, et ce n'est pas une coquetterie.**
C'est de ce rapport qu'on copie-colle une justification vers un commentaire de code, un message de
commit ou un ticket. Un littéral écrit ici revient dans les crates par la porte du copier-coller, et
l'audit qui l'y interdit passe au rouge sans que personne ne sache d'où vient la ligne. Même
discipline pour la table des taux de rareté : elle se nomme, elle ne se recopie pas.

**En revanche, aucun motif de littéral ne balaye ce répertoire, et il ne faut pas en poser un.** Le
tableau joint porte une colonne de graine sur 150 000 runs : la ligne de graine 1 600 y **existe**,
nécessairement, et un score ou un or peuvent valoir 1 600 tout autant. Un tel contrôle serait rouge
dès la première campagne, sans qu'aucune valeur n'ait été codée nulle part, et la seule correction
disponible serait de falsifier le livrable. **Un motif de littéral appliqué à un fichier de données
confond une valeur mesurée avec une valeur codée.**

## 7. Portée et limites

Ce rapport mesure le dépôt tel qu'il est à la fin de l'Étape 6, et il faut savoir ce que cela permet.

- **12 reliques**, contre 60 prévues à l'Étape 9. **3 paliers de rareté peuplés** sur quatre : le
  palier légendaire est **vide**, et sa ligne est non mesurable par construction.
- **4 définitions par palier**, dont une ou deux hors du chemin de score selon le palier. Une médiane
  de palier repose donc sur **deux à quatre** valeurs : c'est la limite dominante de ce rapport,
  et c'est elle qui interdit la conversion du § 5.
- **5 gobelets** sur huit, **6 boss** sur quinze.
- Les consommables existent à **1** variante, **sans effet** : la colonne d'achat de consommable
  mesure une case d'étalage, pas un choix.
- Les **mises** ne sont pas mesurables : **2** effets sont implémentés sur six, et la campagne n'en
  couvre que **3** valeurs pour cette raison. La ligne « non mesurable avant l'Étape 10 » se supprimera en une
  minute le jour où la table existera.
- **Aucune victoire** sur 150 000 runs. Toute question dont la réponse est un taux de victoire est
  donc sans réponse : l'écart entre gobelets, la progression des mises, et l'écart entre sondes en
  points de victoire. Ce rapport les remplace par des grandeurs de profondeur, qui discriminent.

**Aucune marge d'erreur n'est produite ici.** Les nombres d'observations sont donnés partout ; une
barre d'erreur sur quatre définitions par palier transformerait une mesure exploratoire en résultat,
et le lecteur suivant citerait le résultat.

**Ces chiffres sont bruyants, et ce sont les premiers du projet.** Un rapport bruyant et honnête vaut
mieux que l'absence de mesure qu'il remplace.
