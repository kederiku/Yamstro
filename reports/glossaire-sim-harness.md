# Section `sim_harness` — à porter au glossaire

> **Ce fichier n'est pas le glossaire.** C'est le bloc à coller dans
> `00 — Glossaire & Conventions de Nommage`, qui vit sur le corpus et prime sur
> tous les autres documents. Il est livré ici pour que le texte soit rédigé,
> relu et versionné avec le code qu'il décrit ; le document normatif, lui, ne se
> modifie que de la main de son propriétaire.

**Deux opérations, et la seconde n'est pas facultative :**

1. **ajouter** la section ci-dessous, **après le § 3** — hors du § 2, qui est la
   table de `core_engine`, et hors du § 3, qui est celle de la couche graphique ;
2. **retirer** du registre § 2.6 la ligne groupée
   `Policy / ShopPolicy / SimRng / HandView`, qui y déménage.

La ligne du § 2.6 **déménage, elle ne se dédouble pas**. Le registre § 2.6 vit à
l'intérieur du § 2, « Types canoniques — `core_engine` » : y laisser les types du
harnais les placerait dans la table du moteur, c'est-à-dire exactement ce que la
première phrase de la section ci-dessous interdit. Le corpus tient la règle
« un registre, un endroit » ; un nom présent aux deux endroits est un défaut,
pas une redondance de confort.

---

## 8. Types du harnais de simulation — `sim_harness`

**Ces types ne franchissent jamais la frontière de `core_engine`**,
et **aucune crate de jeu ne les importe** — à une seule exception, nommée plus bas.

Le harnais est un **instrument de mesure**, pas une brique du jeu : il dépend du
moteur, jamais l'inverse, et il ne dépend d'aucune crate de jeu. C'est cette
frontière qui rend cent cinquante mille runs jouables en une seconde, le moteur
graphique n'entrant jamais dans son arbre de dépendances normal.

**L'unique exception** est une `[dev-dependencies]` de la crate d'interface vers
le harnais, posée pour le seul test d'accord qui vérifie que les deux
orchestrations — celle du jeu et celle du harnais — commettent la même suite de
scores. Une dépendance de développement n'entre pas dans l'arbre normal, et le
critère de frontière tient. **Une seconde dépendance de ce genre ferait de
l'instrument une brique du jeu**, et le jeu embarquerait le parallélisme, la
ligne de commande et le tableur.

| Identifiant canonique | Forme | Fichier | Rôle |
| :-- | :-- | :-- | :-- |
| `Policy` | trait | `policy/mod.rs` | Ce qui décide d'une main. Reçoit une vue en lecture seule et le générateur de l'instrument ; ne consomme **jamais** un flux de la run. |
| `ShopPolicy` | trait | `policy/mod.rs` | Ce qui décide d'une visite en boutique. Elle choisit, la boucle applique. |
| `SimRng` | struct | `rng.rs` | Le **cinquième flux**, propre au harnais. Dérivé de la graine du run, il ne touche jamais le générateur de la run, qui reste à quatre flux. |
| `HandView` | struct | `view.rs` | Ce qu'une sonde de main voit : dés, évaluations, manche, relances restantes, niveaux de figure, inventaire. En lecture seule. |
| `ShopView` | struct | `view.rs` | Ce qu'une sonde d'achat voit : étalage, or, inventaire, configuration. Ni prix ni rareté recopiés — ils se lisent par les fonctions du moteur. |
| `SimConfig` | struct | `config.rs` | Ce qu'une campagne couvre : runs, graine de base, gobelets, mises, sondes, fils. Ne porte aucune option de sortie. |
| `SimSession` | struct | `state.rs` | L'état d'un run côté harnais. Tient la place de la session de run du jeu, que le harnais ne peut pas importer. |
| `SimHand` | struct | `state.rs` | L'état d'une main côté harnais. Même raison. |
| `HandDecision` | enum | `view.rs` | La décision d'une sonde de main : relancer en conservant des dés, ou soumettre une figure. |
| `LockMask` | struct | `view.rs` | La liste des dés **à conserver**, adressés par identifiant. **Ce n'est pas un masque de bits**, malgré son nom. |
| `ShopAction` | enum | `view.rs` | Une action d'achat : acheter, revendre, relancer l'étalage. |
| `GreedyPolicy` | struct | `policy/greedy.rs` | Sonde gloutonne : le meilleur score immédiat, sans considération de grille. |
| `GridAwarePolicy` | struct | `policy/grid_aware.rs` | Sonde attentive à la grille consommable. **C'est la mesure de l'ADR-001** : son écart avec la sonde gloutonne est la seule mesure objective de la profondeur qu'apporte la grille. |
| `RandomPolicy` | struct | `policy/random.rs` | Le **témoin**. Tire uniformément parmi les figures disponibles et ne relance jamais : c'est la borne inférieure qui valide l'instrument avant le jeu. |
| `BudgetShopPolicy` | struct | `policy/shop.rs` | Sonde d'achat au budget : prix croissant, sans conversion entre paliers de rareté. |
| `SynergyShopPolicy` | struct | `policy/shop.rs` | Sonde d'achat à plan : privilégie l'archétype que les dés de départ dessinent. |
| `RunOutcome` | struct | `outcome.rs` | La ligne de tableau d'un run. Une ligne est identifiée par le quintuplet graine, gobelet, mise, sonde de main, sonde d'achat — **jamais par la graine seule**, les cellules rejouant les mêmes graines. |
| `RunTrace` | struct | `trace.rs` | Le journal d'un run unique, produit sous le mode de journal seul. Un fait par ligne, comparable, et la somme de ses pas reconstitue le score. |
| `RelicStats` | struct | `report.rs` | Les trois taux d'une relique — apparition, achat, conservation — et sa contribution moyenne. Le taux de conservation se rapporte aux runs **qui l'ont vue**, pas à celles qui l'ont achetée. |
| `BalanceReport` | struct | `report.rs` | Le rapport agrégé d'une campagne, en arithmétique **entière** de bout en bout. Les médianes y sont des statistiques d'ordre, jamais des moyennes. |

**Conventions de la section.**

- Aucun flottant, nulle part — **pas même au formatage**. Les taux se comptent en
  dix-millièmes entiers ; une comparaison flottante ferait diverger un verdict
  d'une plateforme à l'autre sans que personne ne sache laquelle ment.
- Aucune table associative, ordonnée ou non. L'identifiant de relique ne dérive
  pas l'ordre — c'est une entrée du fichier des API absentes —, et le harnais
  emploie des listes de paires itérées sur le catalogue.
- Le générateur de la run reste à **quatre** flux. Un cinquième champ ajouté au
  moteur serait une modification du moteur.
- Rien n'est écrit en dur : le harnais itère les catalogues, et couvrira les
  soixante reliques et les huit gobelets de l'Étape 9 sans qu'une ligne change.
