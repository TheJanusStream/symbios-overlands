//! What the avatar editor's 3-D gizmo is aimed at (#1161).
//!
//! The editor can aim at three different kinds of thing — a node of a
//! construction-kit body's visuals tree (#823), a whole worn prop moved
//! through its offset (#1062), and one part inside a worn prop's own tree
//! (#1098) — and the rule that governs them is that **at most one of them
//! is aimed at a time**. That rule used to be spelled out by hand: three
//! parallel `Option` fields, and fourteen methods on
//! [`AvatarEditorState`](super::AvatarEditorState) that each remembered to
//! clear the other two. Two of the #1103 bugs were a new selection kind
//! (#1098's part) that the collapse-deselect and scene-miss paths had never
//! been told about.
//!
//! Here the exclusivity is a property of the type instead: one field
//! holding one variant, so an assignment *is* the mutex and a fourth kind
//! cannot be added without the compiler walking every reader to it.

/// The avatar editor's single gizmo aim.
///
/// Constructed only through
/// [`AvatarEditorState::aim`](super::AvatarEditorState::aim), which is the
/// one place the outgoing target's tree-row highlight is dropped — widget
/// state that lives *beside* the aim rather than in it.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum GizmoTarget {
    /// Nothing is aimed at: no gizmo is attached, and none of the three
    /// avatar holds ([`holds_avatar_still`](super::AvatarEditorState::holds_avatar_still)
    /// and friends) engage.
    #[default]
    None,
    /// A node of the local generator body's visuals tree (#823), addressed
    /// by its child-index chain under the single
    /// [`AvatarVisualsTreeSource::ROOT_NAME`](crate::ui::room::generators::AvatarVisualsTreeSource::ROOT_NAME)
    /// root. An empty path is the root node itself.
    VisualsNode { path: Vec<usize> },
    /// A whole worn prop (#1062), addressed by its attachment record's
    /// rkey — a prop is not a node in any tree, so it is deliberately not
    /// a path. Its gizmo moves the record's `offset`, which lives in the
    /// carrying joint's **rest** frame; see
    /// [`holds_rig_at_rest`](super::AvatarEditorState::holds_rig_at_rest).
    WornProp { rkey: String },
    /// One part inside a worn prop's own generator tree (#1098): the
    /// region-asset editor pointed at the worn copy. Distinct from
    /// [`Self::WornProp`] because the transform it edits is relative to
    /// the item root, which rides the joint wherever the animation has put
    /// it; see [`holds_rig_pose`](super::AvatarEditorState::holds_rig_pose).
    WornPart { rkey: String, path: Vec<usize> },
}

impl GizmoTarget {
    /// True while a gizmo is aimed at anything at all. The question the
    /// freeze gates and the release paths ask.
    pub fn is_aimed(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// The visuals-tree path, if that is what is aimed at.
    pub fn visuals_path(&self) -> Option<&[usize]> {
        match self {
            Self::VisualsNode { path } => Some(path),
            _ => None,
        }
    }

    /// The worn prop's rkey, if a **whole** prop is aimed at. A part
    /// selection is not one: the two carry different rig holds, and every
    /// caller of this wants the offset gizmo's subject.
    pub fn worn_prop(&self) -> Option<&str> {
        match self {
            Self::WornProp { rkey } => Some(rkey),
            _ => None,
        }
    }

    /// The `(rkey, path)` of the aimed worn-prop part, if that is what is
    /// aimed at.
    pub fn worn_part(&self) -> Option<(&str, &[usize])> {
        match self {
            Self::WornPart { rkey, path } => Some((rkey, path)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the type exists for: an aim answers exactly one of the
    /// three questions, so no combination of them can ever be true at once.
    /// Under the three parallel `Option`s this was an invariant maintained
    /// by hand at fourteen call sites, and #1103 bugs 1 and 3 were two
    /// places that had not been told about a third kind.
    #[test]
    fn an_aim_answers_exactly_one_of_the_three_questions() {
        let rkey = String::from("3jzfcijpj2z2a");
        let all = [
            GizmoTarget::None,
            GizmoTarget::VisualsNode { path: vec![1, 0] },
            GizmoTarget::WornProp { rkey: rkey.clone() },
            GizmoTarget::WornPart {
                rkey,
                path: vec![2],
            },
        ];
        for target in all {
            let answered = [
                target.visuals_path().is_some(),
                target.worn_prop().is_some(),
                target.worn_part().is_some(),
            ]
            .into_iter()
            .filter(|yes| *yes)
            .count();
            assert_eq!(
                answered,
                usize::from(target.is_aimed()),
                "{target:?} answered {answered} of the three accessors"
            );
        }
    }

    /// A worn PART is not a worn PROP (#1106): they hold the rig
    /// differently — the prop pins the bind pose, the part pins whatever
    /// pose the body is in — so an accessor that conflated them would
    /// re-pose the body under a part the user just picked.
    #[test]
    fn a_part_is_not_the_prop_that_carries_it() {
        let target = GizmoTarget::WornPart {
            rkey: String::from("3jzfcijpj2z2a"),
            path: vec![0],
        };
        assert_eq!(target.worn_prop(), None);
        assert_eq!(target.worn_part(), Some(("3jzfcijpj2z2a", &[0][..])));
    }
}
