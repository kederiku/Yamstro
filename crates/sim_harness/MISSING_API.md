# API manquantes, relevées par le harnais de simulation

**À l'attention du propriétaire des Étapes 1, 2 et 6**, qui décide. Ce fichier
est un relevé, pas une demande de correctif immédiat : chaque entrée nomme le
besoin, la signature souhaitée et l'étape à qui elle appartient.

**Il n'est pas vide, et ce n'est pas un échec.** La règle n°1 de l'étape est que
`core_engine` ne bouge pas : pas une ligne, pas un `pub`, pas une feature. Le
seul recours devant un accès manquant est une entrée ici — **signaler, jamais
contourner**. Un fichier vide obtenu en élargissant une visibilité serait un
échec de l'étape, pas une réussite.

Format : une entrée par manque, chacune portant le besoin en une phrase, la
signature souhaitée en Rust, et l'étape propriétaire.

---

## 1. Le catalogue des gobelets n'a pas de liste ordonnée

`YahtzeeHand`, `BossId` et `ConsumableId` exposent chacun une constante `ALL`
qui énumère leurs variantes dans l'ordre de déclaration. `CupId` n'en a pas. Le
harnais en a besoin pour construire sa matrice par défaut, qui couvre tous les
gobelets.

En attendant, il tient la liste par une **chaîne de succession exhaustive**
(`config::cup_suivant`), dont la non-exhaustivité devient une erreur de
compilation `E0004` nommant la variante oubliée le jour où un gobelet est
ajouté. **Ce n'est pas un contournement** : c'est la garantie mécanique qui
remplace la liste absente, et elle ne recopie aucun compte. Elle disparaît le
jour où la constante existe.

```rust
impl CupId {
    pub const ALL: [CupId; 5] = [ /* … dans l'ordre de déclaration … */ ];
}
```

**Étape propriétaire : 1** — c'est elle qui a posé `CupId` et les trois autres
constantes `ALL`. Le backlog de l'Étape 9 prévoit déjà la liste ordonnée à
TASK-112 ; l'inscrire ici la date et lui donne un demandeur.

---

## 2. Aucune fonction publique n'assemble une définition de manche

Le moteur donne le type de manche, la courbe, le catalogue des boss et leur
tirage. **Il n'expose rien qui les assemble.** L'assemblage vit dans un système
de la crate d'états, que le harnais ne peut pas appeler — il dépend du moteur
graphique.

```rust
pub fn blind_definition(
    ante: u8, kind: BlindType, cup: CupId, stake_level: u8,
    boss: Option<BossDefinition>,
) -> BlindDefinition
```

En attendant, le harnais réassemble tout dans `blind.rs`, **littéraux de
récompense compris** — trois dollars sur la Petite Mise, quatre sur la Grosse,
cinq sur la Boss. **Cette duplication est un risque de divergence assumé** : le
jour où la récompense d'une Mise Boss changera, deux sites seront à corriger, et
rien ne le signalera. Sa fonction a la forme exacte demandée ci-dessus, pour
qu'elle disparaisse d'une seule pièce.

**Étape propriétaire : 6** — c'est elle qui a écrit l'assemblage inline.

---

## 3. La courbe ne se lit pas facteur par facteur

`target_score` est publique et **suffit à jouer** : le harnais n'est pas bloqué.
Mais le document d'étape demande de lire la courbe **facteur par facteur** —
quelle part vient du type de manche, quelle part du gobelet, quelle part du
stake, quelle part du boss —, et les cinq fonctions qui le permettraient sont
visibles de la seule crate du moteur.

```rust
pub fn blind_mult_permille(kind: BlindType) -> u32
pub fn cup_mult_permille(cup: CupId, kind: BlindType) -> u32
pub fn stake_mult_permille(stake_level: u8, ante: u8) -> u32
pub fn boss_mult_permille(modifier: Option<&BlindModifier>) -> u32
```

**Aucune reconstruction locale n'est acceptable**, et c'est la raison d'être de
cette entrée plutôt que d'un contournement : les quatre facteurs redits ici
contiendraient la courbe, donc les deux nombres que le dépôt tient à un seul
endroit — et ils divergeraient au premier calibrage, c'est-à-dire à la première
chose que cette étape existe pour produire.

Le rapport d'équilibrage écrit donc « non décomposable » pour cette question,
plutôt qu'un chiffre reconstruit. Un résultat vide et daté vaut mieux qu'un
résultat faux.

**Étape propriétaire : 6**

---

## 4. L'identifiant de relique ne s'ordonne pas

Le document d'étape impose une table ordonnée des statistiques par relique.
**Une table ordonnée exige une clé ordonnable**, et l'identifiant de relique ne
dérive ni `Ord` ni `PartialOrd` — le type ne compilerait pas tel qu'écrit.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelicId { … }
```

**Le harnais n'ajoute pas le dérivé.** Ce serait une ligne du moteur, donc la
règle n°1 violée — et un dérivé qui, de surcroît, ferait de l'ordre de
déclaration un ordre normatif sans que personne ne l'ait décidé.

Contournement prévu, à l'étape qui construira le rapport : une liste de paires,
bâtie en itérant le catalogue, **dont l'ordre est déjà l'ordre normatif du
projet**. Elle est déterministe par construction, ne demande aucun trait
nouveau, se lit dans le même ordre que le tableau de sortie, et se remplace par
la table ordonnée en une ligne le jour où l'Étape 2 tranche. Le même
raisonnement vaudra pour l'identifiant de gobelet, à qui `Ord` manque de la même
façon.

**Étape propriétaire : 2**
