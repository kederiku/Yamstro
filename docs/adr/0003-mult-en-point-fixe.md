# ADR-003 — Le Mult est en point fixe, jamais en flottant

**Statut :** Accepté · **Résout :** § 1.3 de l'audit · **Impacte :** Étapes 1, 2, 4, 5, 6, 9, 10

## Contexte

Le déterminisme est le pilier n°2 du projet. Un Mult flottant accumulé sur
8 antes (×1,5 puis ×1,75 puis ×2,0…) diverge entre desktop x86, ARM et WASM.
Les tests unitaires deviennent approximatifs, et le partage de graines, que le
déterminisme offrirait gratuitement, devient impossible.

## Décision

`mult: i64` **en centièmes** (150 = ×1,5, 400 = ×4,00).

```
final_score = ((chips as u128 * mult.max(0) as u128 + 50) / 100) as u64
```

- `add_mult(hundredths: i64)`, `multiply_mult(factor_pct: u32)`.
- Toute valeur de design est un multiple de 5 centièmes.
- Les nombres à virgule flottante restent autorisés **uniquement** pour
  l'interpolation d'affichage (`AnimatedNumber`), jamais dans le module de
  score du moteur.
- Le scaling des blinds est calculé en entier via des facteurs en pour-mille.

## Conséquences

- Tous les tests deviennent exacts : `assert_eq!`, et non
  `assert!((a - b).abs() < eps)`.
- Reproductibilité multi-plateforme garantie.
- Une passe de conversion est nécessaire sur toutes les valeurs de reliques
  existantes.

## Notes d'implémentation, hors ADR

Ajoutées par TASK-16, absentes du journal. L'arithmétique entière impose trois
comportements qui se lisent comme des bugs quand on ne les connaît pas.

- `multiply_mult` ajoute 50 sans condition puis divise, et la division entière
  tronque vers zéro. Sur un Mult positif, c'est l'arrondi au plus proche. Sur
  un Mult négatif, tout résultat remonte d'un centième, même sans partie
  fractionnaire : -200 pris à 150 % rend -299 et non -300, et
  `multiply_mult(100)` cesse d'être l'identité. Seule la demie exacte coïncide.
- `multiply_mult` ne connaît que la borne haute en cas de débordement : un Mult
  très négatif ressort à la valeur maximale, donc de signe inverse.
- `final_score` tronque au lieu de saturer si le quotient dépasse la capacité
  du résultat, ce qui suppose des Chips très proches du maximum.

Les deux premiers sont verrouillés par
`test_multiply_mult_known_divergences_on_negative_mult`. **Le troisième n'est
verrouillé par aucun test** : TASK-05 § 3 a délibérément gardé le sien dans le
domaine où la conversion est exacte.

## Fidélité

Contexte, décision et conséquences sont la recopie de l'ADR-003 du journal tenu
sur Drive, qui fait foi. Le titre du journal y nomme littéralement le type
flottant que ce dépôt s'interdit d'écrire. La section « Notes
d'implémentation » n'appartient pas à l'ADR.
