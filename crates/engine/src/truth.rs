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
    use super::*;

    #[test]
    fn truth_tables_match_all_36_cells() {
        use Truth::{False, Invalid, True, Unknown};

        let values = [True, False, Unknown, Invalid];
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
}
