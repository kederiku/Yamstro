//! Arithmétique du score : ScoreContext (arithmétique seule).

/// Terme additif et terme multiplicatif d'un score, en point fixe.
///
/// `mult` est exprimé en centièmes : 400 vaut ×4,00. Aucun flottant n'entre
/// dans ce module. Une accumulation flottante sur huit antes diverge entre
/// x86, ARM et WASM, ce qui rendrait les tests approximatifs et détruirait le
/// partage de graines (ADR-003).
///
/// L'état initial, celui rendu par `Default`, est `{ chips: 0, mult: 0 }`.
/// C'est l'état d'entrée de la passe d'application, avant que la première
/// étape de résolution n'y verse la base de la figure.
///
/// Le plancher de design « le Mult ne descend pas sous ×1,00 » ne vaut qu'à
/// partir de la fin de cette première étape. Avant elle, `mult = 0` est un
/// état parfaitement légitime : la première entrée du journal de l'exemple
/// résolu affiche 30 Chips, un Mult nul et un score nul, parce que les Chips
/// de base sont versées avant le Mult de base. Un contexte qui
/// « corrigerait » ce zéro rendrait le journal faux et le boss qui ramène tout
/// le Mult à un inexprimable. Le plancher appartient aux producteurs d'effets,
/// jamais à ce type.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Le corps ci-dessous est normatif (Étape 2 § 1.2) et se recopie au
    /// caractère près. Le `+ 50` est inconditionnel et la division entière
    /// tronque vers zéro. Sur un Mult positif, c'est bien l'arrondi au plus
    /// proche. Sur un Mult négatif, tout résultat remonte d'un centième, même
    /// lorsque la valeur exacte est entière. Trois conséquences, signalées ici
    /// plutôt que corrigées :
    ///
    /// - -200 pris à 150 % rend -299 là où la valeur exacte vaut -300 ;
    /// - `multiply_mult(100)` cesse d'être l'identité sous zéro : -500 devient
    ///   -499 et -1 devient 0. La demie exacte est le seul cas négatif qui
    ///   coïncide avec l'arrondi au plus proche, ainsi -333 pris à 150 % rend
    ///   bien -499 ;
    /// - un débordement par le bas change de signe. `i64::try_from` échoue des
    ///   deux côtés et `unwrap_or` ne connaît que la borne haute, si bien
    ///   qu'un Mult très négatif ressort à `i64::MAX`.
    ///
    /// Aucun contenu spécifié ne produit un Mult négatif à ce jour, mais
    /// l'API le permet. Voir
    /// `test_multiply_mult_known_divergences_on_negative_mult`.
    pub fn multiply_mult(&mut self, factor_pct: u32) {
        // arrondi au plus proche ; i128 intermédiaire pour ne jamais déborder
        let scaled = (self.mult as i128 * factor_pct as i128 + 50) / 100;
        self.mult = i64::try_from(scaled).unwrap_or(i64::MAX);
    }

    /// Score final : Chips multipliés par Mult, arrondi au plus proche. Un
    /// Mult négatif vaut zéro.
    ///
    /// Limite connue, signalée plutôt que corrigée ici : la conversion finale
    /// en `u64` tronque au lieu de saturer si le quotient dépasse `u64::MAX`,
    /// ce qui suppose des Chips proches du maximum et un Mult au-dessus de
    /// 100. La formule est normative et se recopie au caractère près.
    pub fn final_score(&self) -> u64 {
        // arrondi au plus proche, saturant
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
    fn test_multiply_mult_exact_factor() {
        let mut context = ScoreContext {
            chips: 0,
            mult: 400,
        };

        context.multiply_mult(150);

        assert_eq!(context.mult, 600);
    }

    #[test]
    fn test_multiply_mult_rounds_to_nearest() {
        // La valeur exacte vaut 499,5 : le + 50 précède la division entière
        // et la remonte à 500.
        let mut context = ScoreContext {
            chips: 0,
            mult: 333,
        };

        context.multiply_mult(150);

        assert_eq!(context.mult, 500);
    }

    #[test]
    fn test_multiply_mult_saturates_to_i64_max() {
        // Le quotient vaut 18 446 744 073 709 551 614, hors de portée d'un
        // i64 : la conversion échoue et la borne haute la rattrape. Aucun
        // débordement, aucun panic, y compris en profil debug.
        let mut context = ScoreContext {
            chips: 0,
            mult: i64::MAX,
        };

        context.multiply_mult(200);

        assert_eq!(context.mult, i64::MAX);
    }

    #[test]
    fn test_multiply_mult_known_divergences_on_negative_mult() {
        // Ces deux cas verrouillent les divergences documentées du bloc
        // normatif, pour qu'une correction future de l'Étape 2 § 1.2 fasse
        // tomber ce test au lieu de laisser le comportement dériver.

        // Sous zéro, multiplier par 100 % n'est plus l'identité.
        let mut identity = ScoreContext {
            chips: 0,
            mult: -500,
        };
        identity.multiply_mult(100);
        assert_eq!(identity.mult, -499);

        // La divergence ne tient pas au facteur 100 ni à une demie : la valeur
        // exacte vaut ici -300, sans partie fractionnaire, et le résultat
        // remonte quand même d'un centième.
        let mut general = ScoreContext {
            chips: 0,
            mult: -200,
        };
        general.multiply_mult(150);
        assert_eq!(general.mult, -299);

        // La demie exacte est le seul cas négatif qui coïncide avec l'arrondi
        // au plus proche : -499,5 rend bien -499.
        let mut half = ScoreContext {
            chips: 0,
            mult: -333,
        };
        half.multiply_mult(150);
        assert_eq!(half.mult, -499);

        // Un débordement par le bas ressort à la borne haute, donc de signe
        // inverse.
        let mut overflow = ScoreContext {
            chips: 0,
            mult: i64::MIN,
        };
        overflow.multiply_mult(200);
        assert_eq!(overflow.mult, i64::MAX);
    }

    #[test]
    fn test_default_is_zero() {
        assert_eq!(ScoreContext::default(), ScoreContext { chips: 0, mult: 0 });
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
