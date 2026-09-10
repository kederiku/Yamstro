# ADR-007 — Toute constante de gameplay naît dans `RunConfig`

**Statut :** Accepté · **Résout :** C09, C10, C18, C19, C24 · **Impacte :** toutes les Étapes

## Contexte

Cinq paramètres étaient codés en dur en Étape 1 et 3, puis contredits
ailleurs : 5 dés contre Gobelet du Tricheur à 6 et Gobelet du Polyèdre à
4×D6 + 1×D8 ; 2 relances contre Obsidienne à 1, L'Étau à 1, Gobelet Abandonné à
0 et Stake 4 à −1 ; 5 slots de reliques contre Gobelet de Fortune à 6 ; plafond
d'intérêts à $5 contre une relique à $10 ; dés à 6 faces contre D8.

Le cas Gobelet Abandonné, à 0 relance, combiné au Stake 4, à −1 relance, sur un
`u8`, produit un débordement par le bas : panic en debug, 255 en release.

## Décision

```rust
pub struct RunConfig {
    pub dice_count: u8,
    pub base_rerolls: u8,
    pub relic_capacity: u8,
    pub consumable_capacity: u8,
    pub max_interest: u32,
    pub hands_per_blind: u8,
}
```

`RunConfig` est **dérivée du `CupDeck`** à la création de la run, jamais écrite
au montage d'une manche.

Chaîne d'application explicite et testée, dans cet ordre exact :

```
base(cup) -> stake -> blind_modifier -> relic_modifier
```

`saturating_sub` partout. Test obligatoire : `test_rerolls_underflow_saturates`
(Gobelet Abandonné, Stake 4 et L'Étau donnent 0, pas 255).

## Conséquences

- Un `dice_count` variable rend implémentable le boss *La Meule*, qui retire le
  dé le plus faible à chaque relance : la main est un `Vec`, pas un tableau de
  slots 0 à 4.
- Les figures et l'affichage ne présupposent plus 5 dés.

## Fidélité

Recopie de l'ADR-007 du journal tenu sur Drive, qui fait foi. Une seule
reformulation : le verbe employé par le journal pour décrire *La Meule* est le
terme du jeu de cartes que ce dépôt proscrit.
