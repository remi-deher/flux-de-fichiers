//! Analyse sécurisée et validation déclarative des paramètres personnalisés CLI.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CustomArgsError {
    #[error("guillemet non fermé dans les paramètres personnalisés")]
    UnclosedQuote,
    #[error("caractère de contrôle interdit dans les paramètres personnalisés")]
    ControlCharacter,
    #[error("les paramètres personnalisés doivent commencer par un drapeau (-...)")]
    MustStartWithFlag,
    #[error("option interdite dans les paramètres personnalisés : {0}")]
    ForbiddenFlag(String),
    #[error("chemin absolu ou remontée de répertoire interdit : {0}")]
    ForbiddenPath(String),
}

/// Découpe une chaîne de paramètres en gérant guillemets simples/doubles et échappements.
pub fn parse_custom_args(raw: &str) -> Result<Vec<String>, CustomArgsError> {
    if raw.chars().any(|ch| matches!(ch, '\0' | '\r' | '\n')) {
        return Err(CustomArgsError::ControlCharacter);
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == '\\' && q == '"' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        return Err(CustomArgsError::UnclosedQuote);
                    }
                } else if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    in_token = true;
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        in_token = true;
                    }
                }
                c if c.is_whitespace() => {
                    if in_token {
                        args.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                }
                other => {
                    current.push(other);
                    in_token = true;
                }
            },
        }
    }

    if quote.is_some() {
        return Err(CustomArgsError::UnclosedQuote);
    }
    if in_token {
        args.push(current);
    }
    Ok(args)
}

fn validate_against(args: &[String], forbidden: &[String]) -> Result<(), CustomArgsError> {
    if args.is_empty() {
        return Ok(());
    }
    if !args[0].starts_with('-') {
        return Err(CustomArgsError::MustStartWithFlag);
    }
    for token in args {
        let flag_name = token.split('=').next().unwrap_or(token.as_str());
        if forbidden
            .iter()
            .any(|blocked| flag_name.eq_ignore_ascii_case(blocked))
        {
            return Err(CustomArgsError::ForbiddenFlag(flag_name.to_owned()));
        }
        if is_forbidden_path(token)
            || token
                .split_once('=')
                .is_some_and(|(_, value)| is_forbidden_path(value))
        {
            return Err(CustomArgsError::ForbiddenPath(token.clone()));
        }
    }
    Ok(())
}

fn is_forbidden_path(candidate: &str) -> bool {
    candidate.starts_with('/') || candidate.contains("..") || candidate.starts_with("file:")
}

/// Analyse et valide des paramètres CLI personnalisés d'après la liste d'exclusions du contrat.
pub fn safe_custom_args(raw: &str, forbidden: &[String]) -> Result<Vec<String>, CustomArgsError> {
    let args = parse_custom_args(raw)?;
    validate_against(&args, forbidden)?;
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_quoted_filters_into_single_tokens() -> Result<(), CustomArgsError> {
        let args = safe_custom_args(
            r#"-vf "scale=1280:-1, eq=contrast=1.1" -tune film -bf 3"#,
            &flags(&["-i", "-y"]),
        )?;
        assert_eq!(
            args,
            [
                "-vf",
                "scale=1280:-1, eq=contrast=1.1",
                "-tune",
                "film",
                "-bf",
                "3"
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_unclosed_quotes_paths_and_forbidden_flags() {
        assert_eq!(
            parse_custom_args(r#"-vf "unclosed"#),
            Err(CustomArgsError::UnclosedQuote)
        );
        assert_eq!(
            safe_custom_args("-i /etc/passwd", &flags(&["-i"])),
            Err(CustomArgsError::ForbiddenFlag("-i".into()))
        );
        assert_eq!(
            safe_custom_args("-metadata comment=/etc/shadow", &[]),
            Err(CustomArgsError::ForbiddenPath("comment=/etc/shadow".into()))
        );
        assert_eq!(
            safe_custom_args("--exec-shell sh", &flags(&["--exec-shell"])),
            Err(CustomArgsError::ForbiddenFlag("--exec-shell".into()))
        );
        assert_eq!(
            safe_custom_args("extra_output.mkv", &[]),
            Err(CustomArgsError::MustStartWithFlag)
        );
    }
}
