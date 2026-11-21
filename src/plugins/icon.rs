//! Icônes SVG des plugins (ADR 0024).
//!
//! Un dépôt de plugins peut être tiers : son SVG n'est jamais fait confiance. Il est validé par
//! liste blanche (éléments et attributs de dessin seulement, aucun script, style, lien ni texte)
//! à la synchronisation, puis de nouveau avant chaque service. Le client l'affiche comme masque
//! d'image, jamais comme document, et la route le sert avec une CSP vide.

use super::CatalogEntry;
use crate::tools_cache::ToolCacheManager;
use serde_json::Value;
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// Taille maximale d'une icône : une icône de dessin n'en a pas besoin de plus.
pub const MAX_ICON_BYTES: usize = 16 * 1024;
/// Extension exigée pour le fichier d'icône.
const ICON_EXTENSION: &str = "svg";
/// Espace de noms SVG, seule valeur admise pour `xmlns`.
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const ROOT_ELEMENT: &str = "svg";
const NAMESPACE_ATTRIBUTE: &str = "xmlns";
/// Éléments de dessin admis ; l'élément racine `svg` est traité à part.
const DRAWING_ELEMENTS: [&str; 8] = [
    "g", "path", "circle", "ellipse", "rect", "line", "polyline", "polygon",
];
/// Attributs admis : géométrie, remplissage et trait. Aucun `style`, `href` ni gestionnaire d'événement.
const ALLOWED_ATTRIBUTES: [&str; 29] = [
    "viewBox",
    "width",
    "height",
    "fill",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-opacity",
    "fill-opacity",
    "fill-rule",
    "clip-rule",
    "opacity",
    "transform",
    "d",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "points",
];
/// Caractères interdits dans une valeur d'attribut : aucun lien, entité ni balise n'y a sa place.
const FORBIDDEN_VALUE_CHARACTERS: [char; 5] = [':', '&', '<', '>', '\\'];

/// Pourquoi une icône est refusée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SvgIssue {
    TooLarge,
    Malformed,
    MissingNamespace,
    RootNotSvg,
    NestedRoot,
    TrailingContent,
    Unbalanced,
    ForbiddenMarkup,
    TextContent,
    Element(String),
    Attribute(String),
    AttributeValue(String),
}

impl fmt::Display for SvgIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(formatter, "plus de {MAX_ICON_BYTES} octets"),
            Self::Malformed => formatter.write_str("SVG mal formé"),
            Self::MissingNamespace => formatter.write_str("espace de noms SVG manquant"),
            Self::RootNotSvg => formatter.write_str("l'élément racine n'est pas svg"),
            Self::NestedRoot => formatter.write_str("svg imbriqué"),
            Self::TrailingContent => formatter.write_str("contenu après l'élément racine"),
            Self::Unbalanced => formatter.write_str("balises non équilibrées"),
            Self::ForbiddenMarkup => {
                formatter.write_str("déclaration, commentaire ou instruction interdits")
            }
            Self::TextContent => formatter.write_str("texte interdit"),
            Self::Element(name) => write!(formatter, "élément interdit « {name} »"),
            Self::Attribute(name) => write!(formatter, "attribut interdit « {name} »"),
            Self::AttributeValue(name) => write!(formatter, "valeur interdite pour « {name} »"),
        }
    }
}

/// Valide une icône SVG par liste blanche. `Ok` signifie qu'elle ne peut ni exécuter de code
/// ni charger de ressource externe.
pub fn check_svg_icon(svg: &str) -> Result<(), SvgIssue> {
    if svg.len() > MAX_ICON_BYTES {
        return Err(SvgIssue::TooLarge);
    }
    let mut rest = svg;
    let mut open: Vec<&str> = Vec::new();
    let mut seen_root = false;
    let mut closed_root = false;
    let mut has_namespace = false;
    while let Some(start) = rest.find('<') {
        if !rest[..start].trim().is_empty() {
            return Err(SvgIssue::TextContent);
        }
        let after = &rest[start + 1..];
        let end = after.find('>').ok_or(SvgIssue::Malformed)?;
        let tag = &after[..end];
        rest = &after[end + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            return Err(SvgIssue::ForbiddenMarkup);
        }
        if let Some(closing) = tag.strip_prefix('/') {
            if open.pop() != Some(closing.trim()) {
                return Err(SvgIssue::Unbalanced);
            }
            closed_root = open.is_empty();
            continue;
        }
        if closed_root {
            return Err(SvgIssue::TrailingContent);
        }
        let self_closing = tag.ends_with('/');
        let body = tag.strip_suffix('/').unwrap_or(tag);
        let (name, attributes) = split_tag(body);
        check_element(name, seen_root)?;
        let declares_namespace = check_attributes(attributes, !seen_root)?;
        has_namespace |= declares_namespace;
        seen_root = true;
        if self_closing {
            closed_root = open.is_empty();
        } else {
            open.push(name);
        }
    }
    if !rest.trim().is_empty() {
        return Err(SvgIssue::TextContent);
    }
    if !seen_root {
        return Err(SvgIssue::RootNotSvg);
    }
    if !open.is_empty() || !closed_root {
        return Err(SvgIssue::Unbalanced);
    }
    if !has_namespace {
        return Err(SvgIssue::MissingNamespace);
    }
    Ok(())
}

fn split_tag(body: &str) -> (&str, &str) {
    let body = body.trim();
    match body.find(char::is_whitespace) {
        Some(split) => (&body[..split], &body[split..]),
        None => (body, ""),
    }
}

fn check_element(name: &str, root_seen: bool) -> Result<(), SvgIssue> {
    if !root_seen {
        return if name == ROOT_ELEMENT {
            Ok(())
        } else {
            Err(SvgIssue::RootNotSvg)
        };
    }
    if name == ROOT_ELEMENT {
        return Err(SvgIssue::NestedRoot);
    }
    if DRAWING_ELEMENTS.contains(&name) {
        Ok(())
    } else {
        Err(SvgIssue::Element(name.to_owned()))
    }
}

/// Lit des attributs `nom="valeur"` (guillemets obligatoires) et les valide un à un.
/// Renvoie `true` si l'élément déclare l'espace de noms SVG (`xmlns`, valide sur la racine).
fn check_attributes(mut input: &str, is_root: bool) -> Result<bool, SvgIssue> {
    let mut declares_namespace = false;
    loop {
        input = input.trim_start();
        if input.is_empty() {
            return Ok(declares_namespace);
        }
        let equals = input.find('=').ok_or(SvgIssue::Malformed)?;
        let name = input[..equals].trim();
        let after_equals = input[equals + 1..].trim_start();
        let quote = after_equals
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''))
            .ok_or(SvgIssue::Malformed)?;
        let value_and_rest = &after_equals[quote.len_utf8()..];
        let close = value_and_rest.find(quote).ok_or(SvgIssue::Malformed)?;
        let value = &value_and_rest[..close];
        input = &value_and_rest[close + quote.len_utf8()..];
        if !input.is_empty() && !input.starts_with(char::is_whitespace) {
            return Err(SvgIssue::Malformed);
        }
        check_attribute(name, value, is_root)?;
        declares_namespace |= name == NAMESPACE_ATTRIBUTE;
    }
}

fn check_attribute(name: &str, value: &str, is_root: bool) -> Result<(), SvgIssue> {
    if name == NAMESPACE_ATTRIBUTE {
        return if is_root && value == SVG_NAMESPACE {
            Ok(())
        } else {
            Err(SvgIssue::AttributeValue(name.to_owned()))
        };
    }
    if !ALLOWED_ATTRIBUTES.contains(&name) {
        return Err(SvgIssue::Attribute(name.to_owned()));
    }
    if value.contains(FORBIDDEN_VALUE_CHARACTERS) {
        return Err(SvgIssue::AttributeValue(name.to_owned()));
    }
    Ok(())
}

/// Chemin du fichier d'icône déclaré par un manifeste, résolu dans le dossier du manifeste.
///
/// `None` si l'icône n'est pas déclarée, sort du dossier du dépôt ou n'est pas un `.svg`.
pub fn declared_icon_path(root: &Path, manifest_path: &str, document: &Value) -> Option<PathBuf> {
    let declared = document.get("icon")?.as_str()?.trim();
    let relative = Path::new(declared);
    let is_svg = relative
        .extension()
        .is_some_and(|extension| extension == ICON_EXTENSION);
    if declared.is_empty()
        || relative.is_absolute()
        || !is_svg
        || !ToolCacheManager::is_safe_relative_path(relative)
    {
        return None;
    }
    let directory = Path::new(manifest_path).parent().unwrap_or(Path::new(""));
    if !ToolCacheManager::is_safe_relative_path(directory) {
        return None;
    }
    Some(root.join(directory).join(relative))
}

/// L'entrée déclare-t-elle une icône dont le fichier existe ?
pub fn entry_has_icon(root: &Path, entry: &CatalogEntry, document: Option<&Value>) -> bool {
    document
        .and_then(|value| declared_icon_path(root, &entry.manifest_path, value))
        .is_some_and(|path| path.is_file())
}

/// Lit l'icône d'un manifeste et ne la rend que si elle passe la validation.
pub fn read_valid_icon(root: &Path, manifest_path: &str) -> Option<String> {
    let manifest = std::fs::read_to_string(root.join(manifest_path)).ok()?;
    let document: Value = serde_json::from_str(&manifest).ok()?;
    let path = declared_icon_path(root, manifest_path, &document)?;
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ICON_BYTES as u64 {
        return None;
    }
    let svg = std::fs::read_to_string(&path).ok()?;
    check_svg_icon(&svg).ok().map(|()| svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VALID: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
  <g stroke-linecap="round">
    <path d="M4 4h16v16H4z"/>
    <circle cx="12" cy="12" r="3"></circle>
  </g>
</svg>"#;

    const MINIMAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;

    #[test]
    fn an_icon_without_the_svg_namespace_is_refused() {
        assert_eq!(
            check_svg_icon("<svg viewBox=\"0 0 8 8\"/>"),
            Err(SvgIssue::MissingNamespace)
        );
        assert_eq!(check_svg_icon("<svg/>"), Err(SvgIssue::MissingNamespace));
    }

    #[test]
    fn a_drawing_only_svg_is_accepted() {
        assert_eq!(check_svg_icon(VALID), Ok(()));
        assert_eq!(check_svg_icon(MINIMAL), Ok(()));
        let single_quotes = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 8 8'/>";
        assert_eq!(check_svg_icon(single_quotes), Ok(()));
    }

    #[test]
    fn scripts_handlers_links_and_styles_are_refused() {
        let hostile = [
            "<svg><script>alert(1)</script></svg>",
            r#"<svg onload="alert(1)"></svg>"#,
            r#"<svg><path d="M0 0" onclick="x"/></svg>"#,
            r##"<svg><use href="#a"/></svg>"##,
            r#"<svg><image href="https://x.test/a.png"/></svg>"#,
            "<svg><foreignObject></foreignObject></svg>",
            r#"<svg style="fill:red"></svg>"#,
            "<svg><style>*{fill:red}</style></svg>",
            r#"<svg><a href="javascript:alert(1)"></a></svg>"#,
            r#"<svg><path d="M0 0" fill="url(https://x.test/p)"/></svg>"#,
            r#"<svg><path d="M0 0" fill="javascript:alert(1)"/></svg>"#,
            r#"<svg xmlns="http://evil.test/ns"></svg>"#,
        ];
        for svg in hostile {
            assert!(check_svg_icon(svg).is_err(), "{svg}");
        }
    }

    #[test]
    fn declarations_comments_entities_and_text_are_refused() {
        assert_eq!(
            check_svg_icon(r#"<?xml version="1.0"?><svg/>"#),
            Err(SvgIssue::ForbiddenMarkup)
        );
        assert_eq!(
            check_svg_icon("<!DOCTYPE svg><svg/>"),
            Err(SvgIssue::ForbiddenMarkup)
        );
        assert_eq!(
            check_svg_icon("<svg><!-- c --></svg>"),
            Err(SvgIssue::ForbiddenMarkup)
        );
        assert_eq!(
            check_svg_icon(r#"<svg><path d="&#x4d;0 0"/></svg>"#),
            Err(SvgIssue::AttributeValue("d".into()))
        );
        assert_eq!(
            check_svg_icon("<svg>texte</svg>"),
            Err(SvgIssue::TextContent)
        );
        assert_eq!(check_svg_icon("<svg/>après"), Err(SvgIssue::TextContent));
    }

    #[test]
    fn structure_must_be_a_single_balanced_svg_root() {
        assert_eq!(check_svg_icon(""), Err(SvgIssue::RootNotSvg));
        assert_eq!(check_svg_icon("<g/>"), Err(SvgIssue::RootNotSvg));
        assert_eq!(
            check_svg_icon("<svg><svg/></svg>"),
            Err(SvgIssue::NestedRoot)
        );
        assert_eq!(
            check_svg_icon("<svg/><svg/>"),
            Err(SvgIssue::TrailingContent)
        );
        assert_eq!(check_svg_icon("<svg><g></svg>"), Err(SvgIssue::Unbalanced));
        assert_eq!(check_svg_icon("<svg><g></g>"), Err(SvgIssue::Unbalanced));
        assert_eq!(check_svg_icon("<svg"), Err(SvgIssue::Malformed));
        assert_eq!(
            check_svg_icon("<svg width=24></svg>"),
            Err(SvgIssue::Malformed)
        );
        assert_eq!(
            check_svg_icon(r#"<svg width="24></svg>"#),
            Err(SvgIssue::Malformed)
        );
    }

    #[test]
    fn element_and_attribute_names_are_case_sensitive_and_allow_listed() {
        assert_eq!(
            check_svg_icon("<svg><SCRIPT></SCRIPT></svg>"),
            Err(SvgIssue::Element("SCRIPT".into()))
        );
        assert_eq!(
            check_svg_icon(r#"<svg><path D="M0 0"/></svg>"#),
            Err(SvgIssue::Attribute("D".into()))
        );
        assert_eq!(
            check_svg_icon(r#"<svg><path id="a"/></svg>"#),
            Err(SvgIssue::Attribute("id".into()))
        );
        assert_eq!(
            check_svg_icon(r#"<svg><g xmlns="http://www.w3.org/2000/svg"/></svg>"#),
            Err(SvgIssue::AttributeValue("xmlns".into()))
        );
    }

    #[test]
    fn an_oversized_icon_is_refused() {
        let padding = " ".repeat(MAX_ICON_BYTES);
        let too_large = format!("{MINIMAL}{padding}");
        assert_eq!(check_svg_icon(&too_large), Err(SvgIssue::TooLarge));
        let filler = " ".repeat(MAX_ICON_BYTES - MINIMAL.len());
        let at_limit = format!("{MINIMAL}{filler}");
        assert_eq!(check_svg_icon(&at_limit), Ok(()));
    }

    #[test]
    fn issues_are_described_in_french() {
        assert_eq!(
            SvgIssue::Element("script".into()).to_string(),
            "élément interdit « script »"
        );
        assert!(SvgIssue::TooLarge.to_string().contains("octets"));
    }

    #[test]
    fn declared_icon_stays_inside_the_manifest_folder_and_must_be_svg() {
        let root = Path::new("/catalogue");
        let manifest = "plugins/plex/manifest.json";
        let resolve = |icon: Value| declared_icon_path(root, manifest, &json!({ "icon": icon }));
        assert_eq!(
            resolve(json!("icon.svg")),
            Some(PathBuf::from("/catalogue/plugins/plex/icon.svg"))
        );
        assert_eq!(
            resolve(json!("img/icon.svg")),
            Some(PathBuf::from("/catalogue/plugins/plex/img/icon.svg"))
        );
        for hostile in [
            json!("../icon.svg"),
            json!("/etc/icon.svg"),
            json!("icon.png"),
            json!("icon"),
            json!(""),
            json!(42),
        ] {
            assert_eq!(resolve(hostile.clone()), None, "{hostile}");
        }
        assert_eq!(declared_icon_path(root, manifest, &json!({})), None);
        assert_eq!(
            declared_icon_path(root, "../manifest.json", &json!({ "icon": "a.svg" })),
            None
        );
    }

    #[test]
    fn a_valid_icon_is_read_and_an_unsafe_or_missing_one_is_not() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let directory = root.path().join("plugins/demo");
        std::fs::create_dir_all(&directory)?;
        let manifest = |icon: &str| json!({ "icon": icon }).to_string();
        std::fs::write(directory.join("manifest.json"), manifest("icon.svg"))?;
        std::fs::write(directory.join("icon.svg"), VALID)?;
        let manifest_path = "plugins/demo/manifest.json";
        assert_eq!(
            read_valid_icon(root.path(), manifest_path).as_deref(),
            Some(VALID)
        );

        std::fs::write(directory.join("icon.svg"), "<svg onload=\"x\"/>")?;
        assert_eq!(read_valid_icon(root.path(), manifest_path), None);

        std::fs::write(directory.join("manifest.json"), manifest("absent.svg"))?;
        assert_eq!(read_valid_icon(root.path(), manifest_path), None);
        let unknown = "plugins/inconnu/manifest.json";
        assert_eq!(read_valid_icon(root.path(), unknown), None);
        Ok(())
    }
}
