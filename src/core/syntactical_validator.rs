use crate::core::models::CommandRules;
use crate::core::errors::AppError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use regex::Regex;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
enum InjectionPosition {
    Immediate,
    End,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct Injection {
    position: InjectionPosition,
    tokens: Vec<String>,
    separator: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PullConfig {
    id: String,
    to: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Transition {
    input: String,
    to: String,
    is_regex: bool,
    injection: Option<Injection>,
    option_id: String,
    excludes: Option<Vec<String>>,
    pull: Option<PullConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FsmConfig {
    initial_state: String,
    accepting_states: Vec<String>,
    transitions: HashMap<String, Vec<Transition>>,
}

impl FsmConfig {
    pub fn validate(&self, initial_tokens: &[String]) -> (bool, Vec<String>) {
        let mut current_state = self.initial_state.clone();
        let mut stream: VecDeque<String> = initial_tokens.iter().cloned().collect();
        let mut output_stream: Vec<String> = Vec::new();
        let mut seen_options: HashSet<String> = HashSet::new();
        let mut pending_pull_state: Option<String> = None;

        while let Some(token) = stream.pop_front() {
            if let Some(t) = self.find_matching_transition(&current_state, &token) {
                if !self.check_exclusions(t, &seen_options) {
                    return (false, Vec::new());
                }

                seen_options.insert(t.option_id.clone());

                current_state = pending_pull_state.take().unwrap_or_else(|| t.to.clone());

                let mut processed_token = token.clone();
                self.apply_injection(t, &mut stream, &output_stream, &token, &mut processed_token);
                self.apply_pull(t, &mut stream, &mut pending_pull_state);

                output_stream.push(processed_token);
            } else {
                return (false, Vec::new());
            }
        }

        (self.accepting_states.contains(&current_state), output_stream)
    }

    fn find_matching_transition<'a>(&'a self, current_state: &str, token: &str) -> Option<&'a Transition> {
        let state_transitions = self.transitions.get(current_state)?;
        for t in state_transitions {
            let is_match = if t.is_regex {
                Regex::new(&t.input).map(|re| re.is_match(token)).unwrap_or(false)
            } else {
                t.input == token
            };

            if is_match {
                return Some(t);
            }
        }
        None
    }

    fn check_exclusions(&self, transition: &Transition, seen_options: &HashSet<String>) -> bool {
        if let Some(exclusion_list) = &transition.excludes {
            if exclusion_list.iter().any(|opt| seen_options.contains(opt)) {
                return false;
            }
        }
        true
    }

    fn apply_injection(
        &self,
        transition: &Transition,
        stream: &mut VecDeque<String>,
        output_stream: &[String],
        current_token: &str,
        processed_token: &mut String,
    ) {
        if let Some(inj) = &transition.injection {
            let sep = inj.separator.as_deref().unwrap_or(" ");

            match inj.position {
                InjectionPosition::Immediate => {
                    if sep == " " {
                        for tok in inj.tokens.iter().rev() {
                            let exists = current_token == tok
                                || output_stream.contains(tok)
                                || stream.contains(tok);
                            if !exists {
                                stream.push_front(tok.clone());
                            }
                        }
                    } else {
                        let fused_suffix = inj.tokens.join("");
                        *processed_token = format!("{}{}{}", processed_token, sep, fused_suffix);
                    }
                }
                InjectionPosition::End => {
                    for tok in &inj.tokens {
                        let exists = current_token == tok
                            || output_stream.contains(tok)
                            || stream.contains(tok);
                        if !exists {
                            stream.push_back(tok.clone());
                        }
                    }
                }
            }
        }
    }

    fn apply_pull(
        &self,
        transition: &Transition,
        stream: &mut VecDeque<String>,
        pending_pull_state: &mut Option<String>,
    ) {
        if let Some(pull_cfg) = &transition.pull {
            if let Some(token_str) = self.find_literal_input_for_option_id(&pull_cfg.id) {
                let positions: Vec<usize> = stream
                    .iter()
                    .enumerate()
                    .filter(|(_, tok)| *tok == &token_str)
                    .map(|(i, _)| i)
                    .collect();

                if positions.len() == 1 {
                    stream.remove(positions[0]);
                }
                stream.push_front(token_str);

                if let Some(new_to) = &pull_cfg.to {
                    *pending_pull_state = Some(new_to.clone());
                }
            }
        }
    }

    fn find_literal_input_for_option_id(&self, option_id: &str) -> Option<String> {
        for transitions in self.transitions.values() {
            for t in transitions {
                if t.option_id == option_id && !t.is_regex {
                    return Some(t.input.clone());
                }
            }
        }
        None
    }
}

/// Syntactical validator that parses command layouts and enforces exclusivity rules.
pub struct SyntacticalValidator;

impl SyntacticalValidator {
    /// Creates a new SyntacticalValidator.
    pub fn new() -> Self {
        Self
    }

    /// Validates the array of tokens against the FSM rules, returning the final command string.
    pub fn validate(&self, tokens: &[String], rules: &CommandRules) -> Result<String, AppError> {
        let fsm: FsmConfig = serde_json::from_value(rules.0.clone())
            .map_err(|e| AppError::Validation(format!("Failed to parse FSM rules: {}", e)))?;

        let (is_valid, final_stream) = fsm.validate(tokens);

        if is_valid {
            Ok(final_stream.join(" "))
        } else {
            Err(AppError::Validation("Validation Failed: Disallowed Syntax".to_string()))
        }
    }
}

impl Default for SyntacticalValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transition(
        input: &str,
        to: &str,
        is_regex: bool,
        option_id: &str,
        injection: Option<Injection>,
        excludes: Option<Vec<String>>,
        pull: Option<PullConfig>,
    ) -> Transition {
        Transition {
            input: input.to_string(),
            to: to.to_string(),
            is_regex,
            injection,
            option_id: option_id.to_string(),
            excludes,
            pull,
        }
    }

    #[test]
    fn test_literal_and_regex_match() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition("cmd", "STATE_A", false, "cmd", None, None, None),
            ],
        );
        transitions.insert(
            "STATE_A".to_string(),
            vec![
                make_transition("^-[0-9]+$", "ACCEPTED", true, "regex_opt", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        // Test literal match
        let (valid1, out1) = fsm.validate(&["cmd".to_string(), "-123".to_string()]);
        assert!(valid1);
        assert_eq!(out1, vec!["cmd", "-123"]);

        // Test failed regex match
        let (valid2, _) = fsm.validate(&["cmd".to_string(), "-abc".to_string()]);
        assert!(!valid2);
    }

    #[test]
    fn test_excludes() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![make_transition("cmd", "OPTIONS", false, "cmd", None, None, None)],
        );
        transitions.insert(
            "OPTIONS".to_string(),
            vec![
                make_transition("-x", "OPTIONS", false, "-x", None, Some(vec!["-y".to_string()]), None),
                make_transition("-y", "OPTIONS", false, "-y", None, Some(vec!["-x".to_string()]), None),
                make_transition("end", "ACCEPTED", false, "end", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        // Only -x: valid
        let (valid1, _) = fsm.validate(&["cmd".to_string(), "-x".to_string(), "end".to_string()]);
        assert!(valid1);

        // Only -y: valid
        let (valid2, _) = fsm.validate(&["cmd".to_string(), "-y".to_string(), "end".to_string()]);
        assert!(valid2);

        // Both -x and -y: invalid
        let (valid3, _) = fsm.validate(&["cmd".to_string(), "-x".to_string(), "-y".to_string(), "end".to_string()]);
        assert!(!valid3);
    }

    #[test]
    fn test_pull() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition(
                    "cmd",
                    "OPTIONS",
                    false,
                    "cmd",
                    Some(Injection {
                        position: InjectionPosition::End,
                        tokens: vec!["target".to_string()],
                        separator: None,
                    }),
                    None,
                    None,
                ),
            ],
        );
        transitions.insert(
            "OPTIONS".to_string(),
            vec![
                make_transition(
                    "--pull",
                    "OPTIONS",
                    false,
                    "--pull",
                    None,
                    None,
                    Some(PullConfig {
                        id: "target".to_string(),
                        to: None,
                    }),
                ),
                make_transition("target", "ACCEPTED", false, "target", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        // Target gets pulled to front of target match
        let (valid1, out1) = fsm.validate(&["cmd".to_string(), "--pull".to_string()]);
        assert!(valid1);
        assert_eq!(out1, vec!["cmd", "--pull", "target"]);
    }

    #[test]
    fn test_injection() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition(
                    "cmd",
                    "OPTIONS",
                    false,
                    "cmd",
                    Some(Injection {
                        position: InjectionPosition::Immediate,
                        tokens: vec!["dep1".to_string(), "dep2".to_string()],
                        separator: Some(" ".to_string()),
                    }),
                    None,
                    None,
                ),
            ],
        );
        transitions.insert(
            "OPTIONS".to_string(),
            vec![
                make_transition("dep1", "STATE_2", false, "dep1", None, None, None),
            ],
        );
        transitions.insert(
            "STATE_2".to_string(),
            vec![
                make_transition("dep2", "ACCEPTED", false, "dep2", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        let (valid1, out1) = fsm.validate(&["cmd".to_string()]);
        assert!(valid1);
        assert_eq!(out1, vec!["cmd", "dep1", "dep2"]);
    }

    #[test]
    fn test_injection_fusion() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition(
                    "cmd",
                    "ACCEPTED",
                    false,
                    "cmd",
                    Some(Injection {
                        position: InjectionPosition::Immediate,
                        tokens: vec!["val".to_string()],
                        separator: Some("=".to_string()),
                    }),
                    None,
                    None,
                ),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        let (valid1, out1) = fsm.validate(&["cmd".to_string()]);
        assert!(valid1);
        assert_eq!(out1, vec!["cmd=val"]);
    }

    #[test]
    fn test_injection_deduplication() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition(
                    "cmd",
                    "OPTIONS",
                    false,
                    "cmd",
                    Some(Injection {
                        position: InjectionPosition::End,
                        tokens: vec!["dep".to_string()],
                        separator: None,
                    }),
                    None,
                    None,
                ),
            ],
        );
        transitions.insert(
            "OPTIONS".to_string(),
            vec![
                make_transition(
                    "--add",
                    "OPTIONS",
                    false,
                    "--add",
                    Some(Injection {
                        position: InjectionPosition::End,
                        tokens: vec!["dep".to_string()],
                        separator: None,
                    }),
                    None,
                    None,
                ),
                make_transition("dep", "ACCEPTED", false, "dep", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        let (valid1, out1) = fsm.validate(&["cmd".to_string(), "--add".to_string()]);
        assert!(valid1);
        assert_eq!(out1, vec!["cmd", "--add", "dep"]);

        let (valid2, out2) = fsm.validate(&["cmd".to_string(), "--add".to_string(), "dep".to_string()]);
        assert!(valid2);
        assert_eq!(out2, vec!["cmd", "--add", "dep"]);
    }

    #[test]
    fn test_syntactical_validator_interface() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "START".to_string(),
            vec![
                make_transition("cmd", "ACCEPTED", false, "cmd", None, None, None),
            ],
        );

        let fsm = FsmConfig {
            initial_state: "START".to_string(),
            accepting_states: vec!["ACCEPTED".to_string()],
            transitions,
        };

        let rules = CommandRules(serde_json::to_value(&fsm).unwrap());
        let validator = SyntacticalValidator::new();

        let tokens = vec!["cmd".to_string()];
        let res = validator.validate(&tokens, &rules);
        assert_eq!(res.unwrap(), "cmd");

        let invalid_tokens = vec!["invalid_cmd".to_string()];
        let res_invalid = validator.validate(&invalid_tokens, &rules);
        assert!(matches!(res_invalid, Err(AppError::Validation(_))));
    }
}
