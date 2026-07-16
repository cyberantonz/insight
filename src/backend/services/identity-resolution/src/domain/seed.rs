//! Persons-seed domain: group source-account profiles and resolve each group to
//! a `person_id` — the **write-side** identity resolution (what the read side
//! only looks up). Pure logic, no DB / IO, mirroring the .NET
//! `EmailProfileResolver` + `PersonAssignmentResolver`.

// Built incrementally: the infra/API slices that consume these types land in
// later commits, so allow dead_code until they're wired in.
#![allow(dead_code)]

use std::collections::HashMap;

use uuid::Uuid;

/// Identifies one source-native account: the source instance (`source_type` +
/// `source_id`) plus the account's native id within it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceAccountKey {
    pub source_type: String,
    pub source_id: Uuid,
    pub account_id: String,
}

/// One account folded from the raw input stream: its current email and whether
/// it is closed (inactive / terminated). The raw observations to persist are
/// attached later (build phase, wired with the `ClickHouse` reader) — the
/// resolver only needs the key, email, and closed flag.
#[derive(Debug, Clone)]
pub struct SeedProfile {
    pub account: SourceAccountKey,
    pub latest_email: Option<String>,
    pub is_closed: bool,
}

/// Accounts that resolve to the same person — grouped by current email.
#[derive(Debug, Clone)]
pub struct ProfileGroup {
    pub profiles: Vec<SeedProfile>,
}

/// How a group's `person_id` was decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentKind {
    /// An account in the group was already bound to a person (idempotent reuse).
    ReusedKnown,
    /// The group's email already maps to an existing person.
    LinkedByEmail,
    /// A fresh person was minted for the group.
    Minted,
}

/// A group bound to a person, carrying the accounts that share it.
#[derive(Debug, Clone)]
pub struct PersonAssignment {
    pub person_id: Uuid,
    pub kind: AssignmentKind,
    pub profiles: Vec<SeedProfile>,
}

/// Assignments plus per-branch counters (feed the operation summary).
#[derive(Debug, Default)]
pub struct ResolveOutcome {
    pub assignments: Vec<PersonAssignment>,
    pub reused_known: usize,
    pub linked_by_email: usize,
    pub minted: usize,
    pub skipped_closed: usize,
    pub skipped_no_email: usize,
}

/// Normalize an email for case-insensitive grouping / lookup (ADR-0011: emails
/// are matched case-insensitively). Trim + lowercase. The infra layer must key
/// the `email → person` map with the same normalization.
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Group profiles that share the same current email into one group; profiles
/// with no (or blank) email each become a singleton group. Mirrors the .NET
/// `EmailProfileResolver`.
#[must_use]
pub fn group_by_email(profiles: Vec<SeedProfile>) -> Vec<ProfileGroup> {
    let mut by_email: HashMap<String, Vec<SeedProfile>> = HashMap::new();
    let mut singletons: Vec<ProfileGroup> = Vec::new();

    for profile in profiles {
        match profile
            .latest_email
            .as_deref()
            .map(normalize_email)
            .filter(|e| !e.is_empty())
        {
            Some(email) => by_email.entry(email).or_default().push(profile),
            None => singletons.push(ProfileGroup {
                profiles: vec![profile],
            }),
        }
    }

    let mut groups: Vec<ProfileGroup> = by_email
        .into_values()
        .map(|profiles| ProfileGroup { profiles })
        .collect();
    groups.extend(singletons);
    groups
}

/// Resolve each group to a `person_id` via the .NET four-branch classification,
/// in priority order: reuse an already-bound account (idempotent); else link to
/// the person the group's email already maps to; else mint a fresh person when
/// at least one profile is active; else skip (no email, or all closed). `mint`
/// is injected so tests are deterministic.
#[must_use]
pub fn resolve_assignments(
    groups: Vec<ProfileGroup>,
    known: &HashMap<SourceAccountKey, Uuid>,
    email_to_person: &HashMap<String, Uuid>,
    mut mint: impl FnMut() -> Uuid,
) -> ResolveOutcome {
    let mut out = ResolveOutcome::default();

    for group in groups {
        // 1. Known binding wins — reuse the person for the whole group, even
        //    when the group also has no email (idempotent re-seed).
        if let Some(pid) = group
            .profiles
            .iter()
            .find_map(|p| known.get(&p.account).copied())
        {
            out.reused_known += group.profiles.len();
            out.assignments.push(PersonAssignment {
                person_id: pid,
                kind: AssignmentKind::ReusedKnown,
                profiles: group.profiles,
            });
            continue;
        }

        // The group's email — shared by every profile in an email group;
        // singleton no-email groups have none.
        let email = group.profiles[0]
            .latest_email
            .as_deref()
            .map(normalize_email)
            .filter(|e| !e.is_empty());
        let Some(email) = email else {
            out.skipped_no_email += group.profiles.len();
            continue;
        };

        // 2. Email matches an existing person → link.
        if let Some(&pid) = email_to_person.get(&email) {
            out.linked_by_email += group.profiles.len();
            out.assignments.push(PersonAssignment {
                person_id: pid,
                kind: AssignmentKind::LinkedByEmail,
                profiles: group.profiles,
            });
            continue;
        }

        // 3/4. No binding, no email match — mint only if at least one profile is
        //      active; a wholly-closed group creates no person.
        if group.profiles.iter().any(|p| !p.is_closed) {
            out.minted += group.profiles.len();
            let person_id = mint();
            out.assignments.push(PersonAssignment {
                person_id,
                kind: AssignmentKind::Minted,
                profiles: group.profiles,
            });
        } else {
            out.skipped_closed += group.profiles.len();
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prof(source_type: &str, account_id: &str, email: Option<&str>, closed: bool) -> SeedProfile {
        SeedProfile {
            account: SourceAccountKey {
                source_type: source_type.to_owned(),
                source_id: Uuid::from_u128(1),
                account_id: account_id.to_owned(),
            },
            latest_email: email.map(str::to_owned),
            is_closed: closed,
        }
    }

    /// A minting factory yielding Uuid(1), Uuid(2), … deterministically.
    fn counter() -> impl FnMut() -> Uuid {
        let mut n = 0u128;
        move || {
            n += 1;
            Uuid::from_u128(n)
        }
    }

    #[test]
    fn groups_by_email_case_insensitively_singletons_for_no_email() {
        let groups = group_by_email(vec![
            prof("bamboohr", "1", Some("anna@corp.com"), false),
            prof("slack", "U1", Some("ANNA@corp.com"), false), // same person, diff case
            prof("bamboohr", "2", Some("boris@corp.com"), false),
            prof("zoom", "Z1", None, false), // no email → singleton
        ]);
        assert_eq!(
            groups.len(),
            3,
            "anna(x2 merged) + boris + no-email singleton"
        );
        let anna = groups
            .iter()
            .find(|g| g.profiles.iter().any(|p| p.account.account_id == "U1"));
        assert!(
            anna.is_some_and(|g| g.profiles.len() == 2),
            "case variants merge into one group"
        );
    }

    #[test]
    fn mints_new_person_for_active_unknown_group() {
        let groups = group_by_email(vec![prof("bamboohr", "1", Some("anna@corp.com"), false)]);
        let out = resolve_assignments(groups, &HashMap::new(), &HashMap::new(), counter());
        assert_eq!(out.minted, 1);
        assert_eq!(out.assignments.len(), 1);
        assert_eq!(out.assignments[0].kind, AssignmentKind::Minted);
        assert_eq!(out.assignments[0].person_id, Uuid::from_u128(1));
    }

    #[test]
    fn skips_wholly_closed_and_no_email_groups() {
        let closed = group_by_email(vec![prof("bamboohr", "1", Some("gone@corp.com"), true)]);
        let out = resolve_assignments(closed, &HashMap::new(), &HashMap::new(), counter());
        assert_eq!(out.skipped_closed, 1);
        assert!(out.assignments.is_empty(), "closed accounts never mint");

        let no_email = group_by_email(vec![prof("zoom", "Z1", None, false)]);
        let out2 = resolve_assignments(no_email, &HashMap::new(), &HashMap::new(), counter());
        assert_eq!(out2.skipped_no_email, 1);
        assert!(out2.assignments.is_empty());
    }

    #[test]
    fn reuses_known_account_binding_over_email() {
        let p = prof("bamboohr", "1", Some("anna@corp.com"), false);
        let mut known = HashMap::new();
        known.insert(p.account.clone(), Uuid::from_u128(42)); // already bound
        let mut email_map = HashMap::new();
        email_map.insert("anna@corp.com".to_owned(), Uuid::from_u128(99)); // different person!

        let out = resolve_assignments(group_by_email(vec![p]), &known, &email_map, counter());
        assert_eq!(out.reused_known, 1);
        assert_eq!(out.linked_by_email, 0);
        // Known binding wins over the email map.
        assert_eq!(out.assignments[0].person_id, Uuid::from_u128(42));
        assert_eq!(out.assignments[0].kind, AssignmentKind::ReusedKnown);
    }

    #[test]
    fn links_new_account_to_existing_person_by_email() {
        // A brand-new account (not in `known`) whose email is already known.
        let groups = group_by_email(vec![prof("github", "gh1", Some("Anna@corp.com"), false)]);
        let mut email_map = HashMap::new();
        email_map.insert("anna@corp.com".to_owned(), Uuid::from_u128(7)); // normalized key
        let out = resolve_assignments(groups, &HashMap::new(), &email_map, counter());
        assert_eq!(out.linked_by_email, 1);
        assert_eq!(out.assignments[0].kind, AssignmentKind::LinkedByEmail);
        assert_eq!(out.assignments[0].person_id, Uuid::from_u128(7));
    }

    #[test]
    fn whole_email_group_binds_to_one_person_via_known_member() {
        // Two accounts share an email; only one is already known → the whole
        // group reuses that person (the new account joins the same person).
        let known_acc = prof("slack", "U1", Some("anna@corp.com"), false);
        let new_acc = prof("github", "gh1", Some("anna@corp.com"), false);
        let mut known = HashMap::new();
        known.insert(known_acc.account.clone(), Uuid::from_u128(5));

        let out = resolve_assignments(
            group_by_email(vec![known_acc, new_acc]),
            &known,
            &HashMap::new(),
            counter(),
        );
        assert_eq!(out.assignments.len(), 1);
        assert_eq!(out.reused_known, 2, "both accounts in the group counted");
        assert_eq!(out.assignments[0].person_id, Uuid::from_u128(5));
        assert_eq!(out.assignments[0].profiles.len(), 2);
    }
}
