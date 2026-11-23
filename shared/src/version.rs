//! Comparaison des versions `x.y.z` : mises à jour de l'application et des éléments, versions minimales
//! exigées par une dépendance, un pack ou une opération (ADR 0029).

type Triplet = (u64, u64, u64);

/// `x.y.z`, préfixe `v`, pré-version et métadonnées ignorés.
fn parse(raw: &str) -> Option<Triplet> {
    let clean = raw
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = clean.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// `latest` est-elle plus récente que `current` ? Une version illisible ne l'est jamais.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse(current), parse(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

/// `version` convient-elle à ce qui est exigé ? `^x.y.z` et `x.y.z` sont des minimums de même version
/// majeure ; une exigence absente ou illisible, ou une version illisible, n'est pas un refus.
pub fn satisfies(required: Option<&str>, version: &str) -> bool {
    let minimum = required.and_then(|range| parse(range.trim().trim_start_matches(['^', '='])));
    match (minimum, parse(version)) {
        (Some(minimum), Some(version)) => version.0 == minimum.0 && version >= minimum,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_compared_as_numbers() {
        assert!(is_newer_version("0.41.0", "0.42.0"));
        assert!(is_newer_version("v0.41.0", "0.41.1"));
        assert!(is_newer_version("1.9.0", "1.10.0"));
        assert!(!is_newer_version("0.41.0", "0.41.0"));
        assert!(!is_newer_version("0.42.0", "0.41.9"));
        assert!(!is_newer_version("1.0.0", "dernière"));
    }

    #[test]
    fn a_requirement_is_a_minimum_of_the_same_major() {
        for required in ["^1.1.0", "1.1.0", "=1.1.0"] {
            assert!(satisfies(Some(required), "1.1.0"), "{required}");
            assert!(satisfies(Some(required), "1.3.0"), "{required}");
            assert!(!satisfies(Some(required), "1.0.9"), "{required}");
            assert!(!satisfies(Some(required), "2.0.0"), "{required}");
        }
    }

    #[test]
    fn an_unreadable_requirement_or_version_is_not_a_refusal() {
        assert!(satisfies(None, "1.0.0"));
        assert!(satisfies(Some("*"), "1.0.0"));
        assert!(satisfies(Some(">=3.0.0"), "1.0.0"));
        assert!(satisfies(Some("^3.0.0"), "locale"));
    }
}
