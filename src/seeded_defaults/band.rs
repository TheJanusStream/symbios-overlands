//! Generic inclusive tier-affinity band (#654) — the one definition
//! behind `ProsperityBand` / `EscalationBand` (scene axes) and
//! `OrnatenessBand` / `WearBand` (avatar axes), which were four
//! byte-identical structs. Each tier axis implements [`BandTier`] to
//! supply its `ANY` endpoints and labels; the concrete band names live
//! on as type aliases so call sites are unchanged.

/// A tier axis usable inside a [`Band`]: an ordered `Copy` enum with
/// fixed lowest/highest endpoints and a display label per tier.
pub trait BandTier: Copy + Ord {
    /// The lowest tier — the [`Band::ANY`] lower endpoint.
    const MIN: Self;
    /// The highest tier — the [`Band::ANY`] upper endpoint.
    const MAX: Self;
    /// Human-readable tier name.
    fn label(self) -> &'static str;
}

/// Inclusive tier affinity band an entry (catalogue item, avatar part)
/// advertises: the contiguous span of tiers a room / avatar may have for
/// the entry to be eligible. [`Self::ANY`] (the default) spans every
/// tier, so untagged entries are always eligible. Relies on the tier
/// enum's lowest-first [`Ord`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band<T> {
    lo: T,
    hi: T,
}

impl<T: BandTier> Band<T> {
    /// Every tier — an untagged, always-eligible entry.
    pub const ANY: Self = Self {
        lo: T::MIN,
        hi: T::MAX,
    };

    /// Eligible only at exactly `tier`.
    pub const fn only(tier: T) -> Self {
        Self { lo: tier, hi: tier }
    }

    /// Eligible across the inclusive `lo..=hi` span (caller passes them in
    /// ascending order).
    pub const fn range(lo: T, hi: T) -> Self {
        Self { lo, hi }
    }

    /// Whether a subject at `tier` may use an entry advertising this band.
    pub fn accepts(self, tier: T) -> bool {
        self.lo <= tier && tier <= self.hi
    }

    /// Human-readable span — `"Any"`, a single tier, or `"lo–hi"`.
    pub fn label(self) -> String {
        if self == Self::ANY {
            "Any".to_string()
        } else if self.lo == self.hi {
            self.lo.label().to_string()
        } else {
            format!("{}–{}", self.lo.label(), self.hi.label())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Toy {
        Low,
        Mid,
        High,
    }
    impl BandTier for Toy {
        const MIN: Self = Toy::Low;
        const MAX: Self = Toy::High;
        fn label(self) -> &'static str {
            match self {
                Toy::Low => "Low",
                Toy::Mid => "Mid",
                Toy::High => "High",
            }
        }
    }

    #[test]
    fn band_semantics_match_the_replaced_structs() {
        assert!(Band::<Toy>::ANY.accepts(Toy::Low));
        assert!(Band::<Toy>::ANY.accepts(Toy::High));
        let only = Band::only(Toy::Mid);
        assert!(only.accepts(Toy::Mid) && !only.accepts(Toy::Low) && !only.accepts(Toy::High));
        let range = Band::range(Toy::Low, Toy::Mid);
        assert!(range.accepts(Toy::Low) && range.accepts(Toy::Mid) && !range.accepts(Toy::High));
        assert_eq!(Band::<Toy>::ANY.label(), "Any");
        assert_eq!(only.label(), "Mid");
        assert_eq!(range.label(), "Low–Mid");
    }
}
