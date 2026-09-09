//! Arithmétique du score : ScoreContext (arithmétique seule).

/// Terme additif et terme multiplicatif d'un score, en point fixe.
///
/// `mult` est exprimé en centièmes : 400 vaut ×4,00. Aucun flottant n'entre
/// dans ce module. Une accumulation flottante sur huit antes diverge entre
/// x86, ARM et WASM, ce qui rendrait les tests approximatifs et détruirait le
/// partage de graines (ADR-003).
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScoreContext {
    pub chips: u64,
    pub mult: i64, // en CENTIÈMES : 400 == ×4.00
}

impl ScoreContext {
    /// Ajoute des Chips, en saturant.
    pub fn add_chips(&mut self, amount: u64) {
        self.chips = self.chips.saturating_add(amount);
    }

    /// Ajoute des centièmes de Mult, en saturant. « +4 Mult » s'écrit
    /// `add_mult(400)`.
    ///
    /// Le plancher de design « le Mult ne descend pas sous ×1,00 » n'est pas
    /// rétabli ici : il appartient aux producteurs d'effets, faute de quoi un
    /// boss qui ramène tout le Mult à un deviendrait inexprimable.
    pub fn add_mult(&mut self, hundredths: i64) {
        self.mult = self.mult.saturating_add(hundredths);
    }

    /// Multiplie le Mult par un pourcentage : « ×1,5 » s'écrit
    /// `multiply_mult(150)`. L'unité est le pour-cent, pas le centième de
    /// Mult ; les deux cohabitent dans ce fichier.
    ///
    /// L'arrondi est au plus proche en s'éloignant de zéro, donc symétrique.
    /// Un `+ 50` inconditionnel suivi d'une division qui tronque vers zéro
    /// remonterait tout Mult négatif : `multiply_mult(100)` ne serait alors
    /// plus l'identité, un Mult de -500 devenant -499.
    pub fn multiply_mult(&mut self, factor_pct: u32) {
        let product = i128::from(self.mult) * i128::from(factor_pct);
        let rounded = if product < 0 {
            (product - 50) / 100
        } else {
            (product + 50) / 100
        };

        self.mult = rounded.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    }

    /// Score final : Chips multipliés par Mult, arrondi au plus proche. Un
    /// Mult négatif vaut zéro.
    ///
    /// Limite connue, à signaler à l'Étape 2 plutôt qu'à corriger ici : la
    /// conversion finale en `u64` tronque au lieu de saturer si le quotient
    /// dépasse `u64::MAX`, ce qui suppose des Chips proches du maximum et un
    /// Mult au-dessus de 100. La formule est normative et se recopie au
    /// caractère près.
    pub fn final_score(&self) -> u64 {
        ((self.chips as u128 * self.mult.max(0) as u128 + 50) / 100) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_final_score_rounds_to_nearest() {
        // 23,31 tombe à 23 ; 1,50 monte à 2 ; 1,49 tombe à 1.
        assert_eq!(
            ScoreContext {
                chips: 7,
                mult: 333
            }
            .final_score(),
            23
        );
        assert_eq!(
            ScoreContext {
                chips: 1,
                mult: 150
            }
            .final_score(),
            2
        );
        assert_eq!(
            ScoreContext {
                chips: 1,
                mult: 149
            }
            .final_score(),
            1
        );
    }

    #[test]
    fn test_final_score_is_deterministic() {
        let left = ScoreContext {
            chips: 12_345,
            mult: 678,
        };
        let right = ScoreContext {
            chips: 12_345,
            mult: 678,
        };

        // Égalité au bit près, jamais un epsilon : c'est tout l'intérêt du
        // point fixe et la condition du partage de graines.
        assert_eq!(left.final_score(), right.final_score());
        assert_eq!(left.final_score(), 83_699);
    }

    #[test]
    fn test_final_score_saturates_on_large_chips() {
        // Vérifie l'absence de débordement du calcul intermédiaire : le produit
        // vaut 1 844 674 407 370 955 161 500, hors de portée d'un u64 mais pas
        // d'un u128. Le quotient, lui, retombe exactement sur u64::MAX.
        let context = ScoreContext {
            chips: u64::MAX,
            mult: 100,
        };

        assert_eq!(context.final_score(), u64::MAX);
    }

    #[test]
    fn test_negative_mult_scores_zero() {
        let context = ScoreContext {
            chips: 100,
            mult: -500,
        };

        assert_eq!(context.final_score(), 0);
    }

    #[test]
    fn test_fixed_point_conventions() {
        let mut context = ScoreContext {
            chips: 0,
            mult: 100,
        };

        context.add_mult(400);
        assert_eq!(context.mult, 500);

        context.multiply_mult(150);
        assert_eq!(context.mult, 750);
    }

    #[test]
    fn test_multiply_mult_rounds_symmetrically_on_negative_mult() {
        // Multiplier par 100 % est l'identité, des deux côtés de zéro.
        let mut context = ScoreContext {
            chips: 0,
            mult: -500,
        };
        context.multiply_mult(100);
        assert_eq!(context.mult, -500);

        // -499,5 s'arrondit en s'éloignant de zéro, comme +499,5.
        let mut negative = ScoreContext {
            chips: 0,
            mult: -333,
        };
        negative.multiply_mult(150);
        assert_eq!(negative.mult, -500);

        let mut positive = ScoreContext {
            chips: 0,
            mult: 333,
        };
        positive.multiply_mult(150);
        assert_eq!(positive.mult, 500);
    }

    #[test]
    fn test_score_context_roundtrip_serde() {
        let context = ScoreContext {
            chips: 1_234,
            mult: -250,
        };

        let encoded = serde_json::to_string(&context).expect("sérialisation");
        assert_eq!(encoded, r#"{"chips":1234,"mult":-250}"#);

        let decoded: ScoreContext = serde_json::from_str(&encoded).expect("désérialisation");
        assert_eq!(decoded, context);
    }
}
