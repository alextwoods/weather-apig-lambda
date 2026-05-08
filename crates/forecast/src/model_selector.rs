use crate::models::{EnsembleModel, ENSEMBLE_MODELS};

// ---------------------------------------------------------------------------
// Model selection parsing and validation
// ---------------------------------------------------------------------------

/// The result of parsing and validating a `models` query parameter.
#[derive(Debug)]
pub struct SelectedModels {
    /// References to the selected ensemble models from the registry.
    pub models: Vec<&'static EnsembleModel>,
}

/// Errors that can occur when parsing the `models` query parameter.
#[derive(Debug)]
pub enum ModelSelectionError {
    /// The parameter was present but empty (e.g., `?models=`).
    EmptyParameter,
    /// One or more suffixes were not recognized as valid ensemble model keys.
    UnknownModels(Vec<String>),
}

impl ModelSelectionError {
    /// Formats the error as a user-facing JSON error message.
    pub fn to_error_message(&self) -> String {
        match self {
            ModelSelectionError::EmptyParameter => {
                "The 'models' parameter cannot be empty. Omit it to use all models, or provide a comma-separated list.".to_string()
            }
            ModelSelectionError::UnknownModels(unknown) => {
                let valid: Vec<&str> = ENSEMBLE_MODELS
                    .iter()
                    .map(|m| m.api_key_suffix)
                    .collect();
                format!(
                    "Unknown model(s): '{}'. Valid models: {}",
                    unknown.join("', '"),
                    valid.join(", ")
                )
            }
        }
    }
}

/// Parses and validates the optional `models` query parameter.
///
/// - `None` → all 5 models (backward compatible)
/// - `Some("")` → validation error (empty string)
/// - `Some("ecmwf_ifs025_ensemble,ncep_gefs_seamless")` → those 2 models
/// - Deduplicates repeated suffixes
/// - Rejects unknown suffixes with an error listing the invalid ones
pub fn parse_model_selection(
    models_param: Option<&str>,
) -> Result<SelectedModels, ModelSelectionError> {
    let input = match models_param {
        None => {
            return Ok(SelectedModels {
                models: ENSEMBLE_MODELS.iter().collect(),
            });
        }
        Some(s) => s,
    };

    if input.is_empty() {
        return Err(ModelSelectionError::EmptyParameter);
    }

    // Split on commas, trim whitespace, and deduplicate while preserving order
    let mut seen = Vec::new();
    let mut suffixes = Vec::new();
    for raw in input.split(',') {
        let suffix = raw.trim();
        if !suffix.is_empty() && !seen.contains(&suffix) {
            seen.push(suffix);
            suffixes.push(suffix);
        }
    }

    // If all tokens were empty after trimming (e.g., ",,,"), treat as empty
    if suffixes.is_empty() {
        return Err(ModelSelectionError::EmptyParameter);
    }

    // Validate each suffix against the registry
    let mut unknown = Vec::new();
    let mut models = Vec::new();
    for suffix in &suffixes {
        match ENSEMBLE_MODELS.iter().find(|m| m.api_key_suffix == *suffix) {
            Some(model) => models.push(model),
            None => unknown.push(suffix.to_string()),
        }
    }

    if !unknown.is_empty() {
        return Err(ModelSelectionError::UnknownModels(unknown));
    }

    Ok(SelectedModels { models })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_returns_all_models() {
        let result = parse_model_selection(None).unwrap();
        assert_eq!(result.models.len(), 5);
        let suffixes: Vec<&str> = result.models.iter().map(|m| m.api_key_suffix).collect();
        assert!(suffixes.contains(&"ecmwf_ifs025_ensemble"));
        assert!(suffixes.contains(&"ncep_gefs_seamless"));
        assert!(suffixes.contains(&"icon_seamless_eps"));
        assert!(suffixes.contains(&"gem_global_ensemble"));
        assert!(suffixes.contains(&"bom_access_global_ensemble"));
    }

    #[test]
    fn test_single_valid_suffix() {
        let result = parse_model_selection(Some("ecmwf_ifs025_ensemble")).unwrap();
        assert_eq!(result.models.len(), 1);
        assert_eq!(result.models[0].api_key_suffix, "ecmwf_ifs025_ensemble");
        assert_eq!(result.models[0].name, "ECMWF IFS 0.25°");
    }

    #[test]
    fn test_two_valid_suffixes() {
        let result =
            parse_model_selection(Some("ecmwf_ifs025_ensemble,ncep_gefs_seamless")).unwrap();
        assert_eq!(result.models.len(), 2);
        let suffixes: Vec<&str> = result.models.iter().map(|m| m.api_key_suffix).collect();
        assert_eq!(suffixes, vec!["ecmwf_ifs025_ensemble", "ncep_gefs_seamless"]);
    }

    #[test]
    fn test_empty_string_returns_error() {
        let err = parse_model_selection(Some("")).unwrap_err();
        match err {
            ModelSelectionError::EmptyParameter => {}
            _ => panic!("Expected EmptyParameter error"),
        }
        assert!(err
            .to_error_message()
            .contains("cannot be empty"));
    }

    #[test]
    fn test_unknown_suffix_returns_error() {
        let err = parse_model_selection(Some("ecmwf_ifs025_ensemble,fake_model")).unwrap_err();
        match &err {
            ModelSelectionError::UnknownModels(unknown) => {
                assert_eq!(unknown, &vec!["fake_model".to_string()]);
            }
            _ => panic!("Expected UnknownModels error"),
        }
        let msg = err.to_error_message();
        assert!(msg.contains("fake_model"));
        assert!(msg.contains("ecmwf_ifs025_ensemble"));
    }

    #[test]
    fn test_duplicated_suffixes_deduplicated() {
        let result = parse_model_selection(Some(
            "ecmwf_ifs025_ensemble,ecmwf_ifs025_ensemble",
        ))
        .unwrap();
        assert_eq!(result.models.len(), 1);
        assert_eq!(result.models[0].api_key_suffix, "ecmwf_ifs025_ensemble");
    }

    #[test]
    fn test_all_five_suffixes() {
        let input = "ecmwf_ifs025_ensemble,ncep_gefs_seamless,icon_seamless_eps,gem_global_ensemble,bom_access_global_ensemble";
        let result = parse_model_selection(Some(input)).unwrap();
        assert_eq!(result.models.len(), 5);
    }

    #[test]
    fn test_whitespace_trimmed() {
        let result =
            parse_model_selection(Some(" ecmwf_ifs025_ensemble , ncep_gefs_seamless ")).unwrap();
        assert_eq!(result.models.len(), 2);
        assert_eq!(result.models[0].api_key_suffix, "ecmwf_ifs025_ensemble");
        assert_eq!(result.models[1].api_key_suffix, "ncep_gefs_seamless");
    }

    #[test]
    fn test_only_commas_returns_empty_error() {
        let err = parse_model_selection(Some(",,,")).unwrap_err();
        match err {
            ModelSelectionError::EmptyParameter => {}
            _ => panic!("Expected EmptyParameter error"),
        }
    }

    #[test]
    fn test_multiple_unknown_suffixes() {
        let err = parse_model_selection(Some("bad_one,bad_two")).unwrap_err();
        match &err {
            ModelSelectionError::UnknownModels(unknown) => {
                assert_eq!(unknown.len(), 2);
                assert!(unknown.contains(&"bad_one".to_string()));
                assert!(unknown.contains(&"bad_two".to_string()));
            }
            _ => panic!("Expected UnknownModels error"),
        }
    }

    #[test]
    fn test_error_message_empty_parameter() {
        let err = ModelSelectionError::EmptyParameter;
        let msg = err.to_error_message();
        assert_eq!(
            msg,
            "The 'models' parameter cannot be empty. Omit it to use all models, or provide a comma-separated list."
        );
    }

    #[test]
    fn test_error_message_unknown_models() {
        let err = ModelSelectionError::UnknownModels(vec!["foo_model".to_string()]);
        let msg = err.to_error_message();
        assert!(msg.contains("Unknown model(s): 'foo_model'"));
        assert!(msg.contains("ecmwf_ifs025_ensemble"));
        assert!(msg.contains("bom_access_global_ensemble"));
    }
}
