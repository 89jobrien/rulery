//! Four-valued policy truth operations.

/// Truth state preserving absent and malformed evidence distinctions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Truth {
    /// Predicate is satisfied.
    True,
    /// Predicate is not satisfied.
    False,
    /// Required evidence is absent or indeterminate.
    Unknown,
    /// Evidence exists but violates its contract.
    Invalid,
}

impl Truth {
    /// Every value, in declaration order.
    ///
    /// Exhaustive iteration and test enumeration read this rather than restating the variant list,
    /// so a new variant has exactly one place to be added and every consumer follows. Nothing can
    /// force that edit in stable Rust, but the companion assertions here fail loudly when a variant
    /// reaches `ALL` without reaching the absorption orders, rather than a property test quietly
    /// ceasing to generate it.
    pub const ALL: [Self; 4] = [Self::True, Self::False, Self::Unknown, Self::Invalid];

    /// Applies the normative four-valued conjunction table.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::Invalid, _) | (_, Self::Invalid) => Self::Invalid,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::True, Self::True) => Self::True,
        }
    }

    /// Applies the normative four-valued disjunction table.
    #[must_use]
    pub const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::Invalid, _) | (_, Self::Invalid) => Self::Invalid,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::False, Self::False) => Self::False,
        }
    }

    /// Applies the normative four-valued negation table.
    #[must_use]
    pub const fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
            Self::Invalid => Self::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The total order in which conjunction lets one operand absorb the other.
    ///
    /// `and` is exactly `min` under this order: `False` wins against anything, `Invalid` beats
    /// `Unknown` and `True`, and `Unknown` beats `True`.
    const AND_ORDER: [Truth; 4] = [Truth::False, Truth::Invalid, Truth::Unknown, Truth::True];

    /// The total order in which disjunction lets one operand absorb the other.
    ///
    /// `or` is exactly `max` under this order. It is deliberately *not* the order above:
    /// `Unknown` precedes `Invalid` here, while `Invalid` precedes `Unknown` there. Both are
    /// correct, because the two operations resolve the same pair differently, so no single order
    /// describes both and a refactor that unifies them would change semantics.
    const OR_ORDER: [Truth; 4] = [Truth::False, Truth::Unknown, Truth::Invalid, Truth::True];

    fn rank(order: [Truth; 4], value: Truth) -> usize {
        order
            .into_iter()
            .position(|entry| entry == value)
            .expect("ordered value")
    }

    fn min_of(order: [Truth; 4], left: Truth, right: Truth) -> Truth {
        if rank(order, left) <= rank(order, right) {
            left
        } else {
            right
        }
    }

    fn max_of(order: [Truth; 4], left: Truth, right: Truth) -> Truth {
        if rank(order, left) >= rank(order, right) {
            left
        } else {
            right
        }
    }

    fn strategy() -> impl Strategy<Value = Truth> {
        prop::sample::select(Truth::ALL.to_vec())
    }

    /// Both absorption orders must stay permutations of the full variant set.
    ///
    /// This is the guard against the silent gap a hand-written order invites: add a variant to
    /// `Truth` and forget the orders, and `rank` panics on a value no property test ever
    /// generates. Asserting coverage here turns that into a failure with a clear cause.
    #[test]
    fn the_absorption_orders_cover_every_value() {
        assert_eq!(
            Truth::ALL,
            [Truth::True, Truth::False, Truth::Unknown, Truth::Invalid],
            "the exhaustive tables below are indexed by `ALL`, so its order is part of the contract"
        );
        for order in [AND_ORDER, OR_ORDER] {
            assert_eq!(
                order.len(),
                Truth::ALL.len(),
                "{order:?} must cover every value"
            );
            for value in Truth::ALL {
                assert!(order.contains(&value), "{order:?} omits {value:?}");
            }
        }
    }

    #[test]
    fn truth_tables_match_all_36_cells() {
        use Truth::{False, Invalid, True, Unknown};

        let values = Truth::ALL;
        let and_table = [
            [True, False, Unknown, Invalid],
            [False, False, False, False],
            [Unknown, False, Unknown, Invalid],
            [Invalid, False, Invalid, Invalid],
        ];
        let or_table = [
            [True, True, True, True],
            [True, False, Unknown, Invalid],
            [True, Unknown, Unknown, Invalid],
            [True, Invalid, Invalid, Invalid],
        ];

        for (left_index, left) in values.iter().copied().enumerate() {
            for (right_index, right) in values.iter().copied().enumerate() {
                assert_eq!(left.and(right), and_table[left_index][right_index]);
                assert_eq!(left.or(right), or_table[left_index][right_index]);
            }
        }

        assert_eq!(True.not(), False);
        assert_eq!(False.not(), True);
        assert_eq!(Unknown.not(), Unknown);
        assert_eq!(Invalid.not(), Invalid);

        assert_eq!(Invalid.and(False), False);
        assert_eq!(Invalid.or(True), True);
        assert_ne!(Unknown, Invalid);
    }

    /// The unconditional absorption law does not hold here, and this pins exactly why.
    ///
    /// Disjunction lets the stronger value win, so `Unknown or Invalid` is `Invalid` — and
    /// conjunction cannot hand the weaker operand back once the stronger one is in play. Stating
    /// absorption without this condition produces a property test that fails on this exact pair.
    #[test]
    fn absorption_needs_the_operands_to_be_ordered() {
        assert_eq!(Truth::Unknown.or(Truth::Invalid), Truth::Invalid);
        assert_ne!(
            Truth::Unknown.and(Truth::Unknown.or(Truth::Invalid)),
            Truth::Unknown
        );
    }

    /// Neither operation distributes over the other, and these are the two counterexamples.
    ///
    /// Distributivity needs a single consistent order, and `AND_ORDER` and `OR_ORDER` disagree on
    /// exactly the `Unknown`/`Invalid` pair, so the classic lattice laws are unavailable here. This
    /// is not a defect to be fixed: the specification's tables define this algebra, and a "repair"
    /// would change evaluation semantics. What a future change must do is decide deliberately which
    /// law it is giving up, not discover it from a red build.
    #[test]
    fn neither_operation_distributes_over_the_other() {
        assert_ne!(
            Truth::Unknown.and(Truth::True.or(Truth::Invalid)),
            Truth::Unknown
                .and(Truth::True)
                .or(Truth::Unknown.and(Truth::Invalid))
        );
        assert_ne!(
            Truth::Unknown.or(Truth::False.and(Truth::Invalid)),
            Truth::Unknown
                .or(Truth::False)
                .and(Truth::Unknown.or(Truth::Invalid))
        );
    }

    /// The two absorption orders genuinely disagree, which is the root cause above.
    #[test]
    fn the_two_absorption_orders_disagree() {
        assert!(
            rank(AND_ORDER, Truth::Invalid) < rank(AND_ORDER, Truth::Unknown),
            "conjunction lets Invalid absorb Unknown"
        );
        assert!(
            rank(OR_ORDER, Truth::Unknown) < rank(OR_ORDER, Truth::Invalid),
            "disjunction lets Unknown absorb Invalid"
        );
    }

    proptest! {
                /// Conjunction and disjunction are commutative.
                #[test]
                fn conjunction_and_disjunction_are_commutative(
                    left in strategy(),
                    right in strategy(),
                ) {
                    prop_assert_eq!(left.and(right), right.and(left));
                    prop_assert_eq!(left.or(right), right.or(left));
                }

                /// Both operations associate, so bracketing cannot change an outcome.
                #[test]
                fn conjunction_and_disjunction_associate(
                    first in strategy(),
                    second in strategy(),
                    third in strategy(),
                ) {
                    prop_assert_eq!(first.and(second).and(third), first.and(second.and(third)));
                    prop_assert_eq!(first.or(second).or(third), first.or(second.or(third)));
                }

                /// Negation is an involution, including on the two values it fixes.
                #[test]
                fn negation_is_an_involution(value in strategy()) {
                    prop_assert_eq!(value.not().not(), value);
                }

                /// De Morgan holds in both directions across every pair.
                #[test]
                fn de_morgan_holds_across_every_pair(left in strategy(), right in strategy()) {
                    prop_assert_eq!(left.and(right).not(), left.not().or(right.not()));
                    prop_assert_eq!(left.or(right).not(), left.not().and(right.not()));
                }

                /// Each operation is idempotent.
                #[test]
                fn both_operations_are_idempotent(value in strategy()) {
                    prop_assert_eq!(value.and(value), value);
                    prop_assert_eq!(value.or(value), value);
                }

                /// `True` is the identity of conjunction and `False` the identity of disjunction.
                #[test]
                fn decisive_values_are_identity_elements(value in strategy()) {
                    prop_assert_eq!(value.and(Truth::True), value);
                    prop_assert_eq!(value.or(Truth::False), value);
                }

                /// `False` annihilates conjunction and `True` annihilates disjunction.
                #[test]
                fn decisive_values_annihilate_the_other_operation(value in strategy()) {
                    prop_assert_eq!(value.and(Truth::False), Truth::False);
                    prop_assert_eq!(value.or(Truth::True), Truth::True);
                }

                /// Absorption holds when the left operand survives the disjunction, and the result is
                /// otherwise always one of the two operands.
                #[test]
                fn absorption_holds_when_the_left_operand_dominates(
                    left in strategy(),
                    right in strategy(),
                ) {
                    let disjoined = left.or(right);
                    let conjunctive = left.and(disjoined);
                    if disjoined == left {
                        prop_assert_eq!(conjunctive, left);
                    }
                    prop_assert!(
                        conjunctive == left || conjunctive == right,
                        "conjunction with a disjunction must resolve to one of the two operands"
                    );
                }

    /// Neither operation ever invents a value that was not supplied.
                ///
                /// A policy engine may read this as: a conjunct or disjunct can only ever select among the
                /// evidence it was handed, never substitute a verdict of its own.
                #[test]
                fn binary_results_never_invent_a_value(
                    left in strategy(),
                    right in strategy(),
                ) {
                    let conjunctive = left.and(right);
                    let disjunctive = left.or(right);
                    prop_assert!(conjunctive == left || conjunctive == right);
                    prop_assert!(disjunctive == left || disjunctive == right);
                }

                /// Conjunction is `min` under `AND_ORDER`.
                #[test]
                fn conjunction_is_the_minimum_under_its_own_order(
                    left in strategy(),
                    right in strategy(),
                ) {
                    prop_assert_eq!(left.and(right), min_of(AND_ORDER, left, right));
                }

                /// Disjunction is `max` under `OR_ORDER`.
                #[test]
                fn disjunction_is_the_maximum_under_its_own_order(
                    left in strategy(),
                    right in strategy(),
                ) {
                    prop_assert_eq!(left.or(right), max_of(OR_ORDER, left, right));
                }

                /// Conjunction is monotone in both operands under `AND_ORDER`.
                ///
                /// Making either operand more absorbing can never make the result less absorbing. That is
                /// the soundness property a policy engine leans on: once evidence is known to be missing or
                /// malformed, no later conjunct can talk the outcome back up.
                #[test]
                fn conjunction_is_monotone_under_its_own_order(
                    weaker in strategy(),
                    stronger in strategy(),
                    other in strategy(),
                ) {
                    if rank(AND_ORDER, weaker) <= rank(AND_ORDER, stronger) {
                        prop_assert!(
                            rank(AND_ORDER, weaker.and(other)) <= rank(AND_ORDER, stronger.and(other))
                        );
                        prop_assert!(
                            rank(AND_ORDER, other.and(weaker)) <= rank(AND_ORDER, other.and(stronger))
                        );
                    }
                }

                /// Disjunction is monotone in both operands under `OR_ORDER`.
                #[test]
                fn disjunction_is_monotone_under_its_own_order(
                    weaker in strategy(),
                    stronger in strategy(),
                    other in strategy(),
                ) {
                    if rank(OR_ORDER, weaker) <= rank(OR_ORDER, stronger) {
                        prop_assert!(
                            rank(OR_ORDER, weaker.or(other)) <= rank(OR_ORDER, stronger.or(other))
                        );
                        prop_assert!(
                            rank(OR_ORDER, other.or(weaker)) <= rank(OR_ORDER, other.or(stronger))
                        );
                    }
                }

                /// Negation never moves a value across the decisive/indeterminate boundary.
                #[test]
                fn negation_moves_only_decisive_values(value in strategy()) {
                    let decisive = matches!(value, Truth::True | Truth::False);
                    prop_assert_eq!(
                        matches!(value.not(), Truth::True | Truth::False),
                        decisive,
                        "negation moved a value across the decisive/indeterminate boundary"
                    );
                }
            }
}
