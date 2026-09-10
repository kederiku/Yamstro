# ADR-002 — Les reliques sont des données, pas des objets-traits

**Statut :** Accepté · **Résout :** C06, borrow-checker Miroir Double · **Impacte :** Étapes 2, 3, 5, 9, 10

## Contexte

L'Étape 2 signait le pipeline avec `relics: &[Box<dyn Relic>]`, l'Étape 3
stockait `relic_slots: Vec<Entity>`, l'Étape 5 stockait
`slots: Vec<Option<RelicInstance>>` et l'Étape 10 exigeait une sérialisation
serde intégrale de l'inventaire. Trois représentations pour une même donnée, et
un objet-trait qui n'est ni `Serialize` ni `Reflect` sans `typetag`.

S'y ajoutait un mur de borrow-checker : la relique Miroir Double re-déclenche
l'effet de sa voisine de gauche, ce qui impose un second emprunt mutable d'un
autre élément du même slice pendant l'itération `&mut`.

## Décision

Abandon de l'objet-trait. Modèle données + fonctions :

```rust
// Données : sérialisables, réfléchissables, déterministes
pub struct RelicInstance {
    pub uid: u32,
    pub def: RelicId,
    pub state: RelicState,
}

pub enum RelicState {
    None,
    Counter(u32),
    Perishable { rounds_left: u8 },
    Disabled,
}

// Comportement : statique, sans allocation
pub fn effects_for(def: RelicId, hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]>;
```

Pipeline de scoring en deux phases :

- **Phase A** (pure, sans mutation) : chaque relique produit une
  `SmallVec<[ScoreEffect; 2]>` à partir d'un `&TriggerCtx` en lecture seule.
  Aucun emprunt mutable, donc Miroir Double peut lire les effets de sa voisine
  sans conflit.
- **Phase B** (application) : application séquentielle des effets sur le
  `ScoreContext` et génération des `ScoreStep`.

Un seul point de vérité : `RelicInventory` en `Resource`, contenant
`slots: Vec<Option<RelicInstance>>`. Les entités Bevy ne portent qu'un
`RelicSlotUI(u8)` d'affichage — aucune donnée de jeu n'est stockée dans une
entité.

## Conséquences

- Sérialisation serde triviale, `Reflect` gratuit, tests possibles sans ECS.
- L'ordre du journal de score devient déterministe par construction.
- La table de hachage indexée par chaîne qui portait l'état disparaît : typage
  par chaîne, ordre d'itération non déterministe, et coût de sérialisation.
- Ajouter une relique se réduit à une variante de `RelicId` plus un bras de
  `match` dans `effects_for`. Le compilateur signale toute relique non
  implémentée.

## Notes d'implémentation, hors ADR

Ajoutées par TASK-17, absentes du journal.

- `RelicId` est **unit-only** : aucune variante ne porte de donnée. C'est ce qui
  le garde `Copy`, donc stockable dans un `StepSource` lui aussi `Copy`, donc
  compatible avec la promesse « `SmallVec` sans allocation » de la phase A. Une
  seule variante à données ferait tomber toute la chaîne. Les paramètres d'une
  relique vivent dans `effects_for`.
- Hors build de test, `RelicId` est **inhabité** jusqu'à l'Étape 5 : seules
  trois fixtures existent, sous `#[cfg(test)]`. Vérifié à TASK-17, cet enum vide
  passe `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize` et `Reflect`
  sans le moindre aménagement, avec et sans la feature `bevy`.
- `RelicInventory.slots` est un `Vec` public sans invariant tenu par le type,
  alors que le numéro de slot rendu par `iter_slots` est un `u8`. Au-delà de 256
  slots, deux slots distincts se présenteraient sous le même numéro.
  `RunConfig.relic_capacity` étant un `u8`, la configuration ne peut pas
  l'enfreindre, mais un `Vec` construit à la main ou relu d'une sauvegarde le
  peut : un `debug_assert!` le rend bruyant plutôt que silencieux.

## Fidélité

Contexte, décision et conséquences sont la recopie de l'ADR-002 du journal tenu
sur Drive, qui fait foi. Deux formulations sont contournées, non modifiées :
l'objet-trait et la table de hachage y sont nommés par leur écriture Rust, que
la DoD de TASK-17 interdit dans `crates/core_engine/src/relics/`. La section
« Notes d'implémentation » n'appartient pas à l'ADR.
