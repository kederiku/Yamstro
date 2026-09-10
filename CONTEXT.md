# Yamstro

Roguelike de Yams : une run enchaîne des antes, et chaque blind se gagne en
atteignant un score cible avec des dés, des reliques et des gobelets.

Ce fichier est un glossaire, rien d'autre. La source faisant foi reste le
dossier Drive `Yamstro/Yamstro v2` (glossaire, journal ADR, documents d'étape) ;
ce qui suit en est un miroir de travail, destiné à rester lisible hors ligne.

## Score

**Chips** :
Terme additif du score. Provient de la base de la figure, des dés
comptabilisés, des sceaux et des reliques.

**Mult** :
Terme multiplicatif du score. Le score vaut Chips × Mult, et cette formule ne
se remplace jamais : un effet de design formulé « ×3 le score total » se
réécrit en « ×3,00 Mult ».
_Avoid_ : multiplicateur, coefficient

**Plancher du Mult** :
Règle de design voulant que le Mult ne descende jamais sous ×1,00, sauf
mention explicite d'un boss. Elle vaut à partir de la fin de la première étape
de résolution et appartient aux producteurs d'effets, jamais au type qui porte
l'arithmétique.

**Journal de score** :
Suite ordonnée des mutations du score produites par une résolution, une entrée
par effet appliqué. C'est lui que la couche de rendu dépile pour animer le
décompte.
_Avoid_ : historique, trace, log

## Dés et figures

**Figure** :
L'une des treize combinaisons du Yams retenues par le jeu, chacune dotée d'une
base en Chips et en Mult.
_Avoid_ : combinaison, main

**Niveau de figure** :
Palier d'amélioration d'une figure, de 1 à 10. Chaque niveau au-delà du
premier ajoute +15 Chips et +1 Mult à sa base.

**Main** :
Ensemble des dés en jeu pour une tentative de score.

**Relance** :
Nouveau lancer d'une partie des dés de la main, dans la limite allouée par la
run.

**Dé comptabilisé** :
Dé retenu par la figure et qui verse donc sa valeur faciale, son modificateur
et son sceau dans le score.
_Avoid_ : dé de score, `ScoringDie` (nom proscrit et gardé par la CI sur le
code de la crate : il entrerait en collision avec un composant du moteur de
rendu)

**Dé écarté** :
Dé de la main que la figure ne retient pas.
_Avoid_ : le terme français usuel du jeu de cartes, proscrit dans tout le
dépôt ; la CI en garde le code de la crate

**Sceau** :
Marque apposée sur un dé, qui produit un effet supplémentaire à un déclencheur
qui lui est propre : dé comptabilisé, dé relancé, ou dé encore en jeu à la fin
d'une manche gagnée.

**Modificateur de dé** :
Altération permanente de la contribution d'un dé, distincte du sceau.

**Gobelet** :
Configuration de départ d'une run : les dés qu'elle fournit et les valeurs de
gameplay qu'elle fixe. Le choix du gobelet est le premier arbitrage d'une run.

## Progression

**Run** :
Une partie complète, du premier ante à la victoire ou à la défaite. Elle est
entièrement déterminée par sa graine.

**Ante** :
Palier de progression d'une run. Chacun contient exactement trois blinds et
relève leur score cible ; une run en compte huit.

**Blind** :
Une manche à gagner : un score cible, un nombre de mains alloué, et
éventuellement un modificateur qui altère les règles.

**Boss** :
Blind terminal d'un ante, porteur d'un modificateur nommé. Seul un boss peut
déroger au plancher du Mult.

**Relique** :
Objet permanent qui produit des effets de score à des points de déclenchement
donnés. Les reliques sont des données, jamais des objets-trait.

**Déclencheur** :
Moment du tour où les reliques et les sceaux sont consultés. Il y en a quatre,
aux noms canoniques `OnRoll`, `OnScoringDie`, `OnHandScored` et `OnRoundEnd`.
Un déclencheur nomme un moment, jamais un effet : ce qu'une relique produit à
ce moment est décidé ailleurs.
`OnScoringDie` contient le nom d'un type proscrit plus haut, sans en être un :
l'un est un moment du tour, l'autre serait un type en collision avec un
composant du moteur de rendu. La garde de la CI est ancrée aux limites de mot
pour les distinguer.
_Avoid_ : `RelicHook`, `RelicEffect`

**Inventaire** :
Suite ordonnée des reliques possédées. L'ordre est strict, de gauche à droite,
et n'est jamais retrié par type d'effet : réordonner l'inventaire change le
score, et c'est la décision de build centrale du jeu.

**Or** :
Monnaie de la run, gagnée en fin de blind et dépensée en boutique.
