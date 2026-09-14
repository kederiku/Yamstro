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
