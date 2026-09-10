# ADR-005 — Ordre strict d'inventaire pour les reliques

**Statut :** Accepté · **Résout :** C05 · **Impacte :** Étapes 2, 5

## Contexte

L'Étape 2 énonçait les deux règles dans la même phrase : « d'abord les additions
(+Chips, puis +Mult), et enfin les multiplications (×Mult) » et « ou dans
l'ordre strict de l'inventaire comme dans Balatro ». L'Étape 5 posait l'ordre
strict d'inventaire. Elles sont mutuellement exclusives.

## Décision

Ordre strict de gauche à droite dans l'inventaire. Aucun tri par type d'action.

## Conséquences

- Le réordonnancement des reliques par le joueur devient une décision
  stratégique réelle — c'est le cœur du build, exactement comme dans Balatro.
  Placer un ×Mult avant ou après un +Mult change le score.
- L'Étape 2 est corrigée : la clause « d'abord les additions, ensuite les
  multiplications » est supprimée.
- Le test `test_relic_reorder_changes_score` devient un test de non-régression
  central.

## Notes d'implémentation, hors ADR

Ajoutées par TASK-17, absentes du journal.

`RelicInventory::iter_slots` parcourt `slots` dans l'ordre du `Vec` et ne filtre
que les slots vides. C'est le seul point de lecture de l'inventaire, et il n'a
le droit de rien trier : c'est là que cet ADR se tient ou s'effondre. Le test
qui le mesure vraiment, `test_relic_reorder_changes_score`, appartient à
TASK-26 ; à TASK-17, rien ne garde encore l'ordre contre un tri qu'on
ajouterait, sinon la relecture.

## Fidélité

Contexte, décision et conséquences sont la recopie de l'ADR-005 du journal tenu
sur Drive, qui fait foi. La section « Notes d'implémentation » n'appartient pas
à l'ADR.
