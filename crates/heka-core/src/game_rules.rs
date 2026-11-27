use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn program_name(program: &str) -> String {
    let trimmed = program.trim();
    let name = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    name.trim().to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRule {
    pub program: String,
    pub profile: String,
}

impl GameRule {
    pub fn new(program: &str, profile: &str) -> Option<Self> {
        let program = program_name(program);
        (!program.is_empty() && !profile.is_empty()).then(|| Self {
            program,
            profile: profile.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct GameRules(Vec<GameRule>);

impl GameRules {
    pub(crate) fn load(value: Option<&Value>) -> Self {
        let mut rules = Self::default();
        let Some(Value::Array(entries)) = value else {
            return rules;
        };
        for entry in entries {
            let program = entry.get("program").and_then(Value::as_str);
            let profile = entry.get("profile").and_then(Value::as_str);
            if let Some((program, profile)) = program.zip(profile) {
                rules.add(program, profile);
            }
        }
        rules
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &GameRule> {
        self.0.iter()
    }

    pub fn find(&self, program: &str) -> Option<&GameRule> {
        let name = program_name(program);
        self.0.iter().find(|rule| rule.program == name)
    }

    pub fn add(&mut self, program: &str, profile: &str) -> bool {
        let Some(rule) = GameRule::new(program, profile) else {
            return false;
        };
        if self.find(&rule.program).is_some() {
            return false;
        }
        self.0.push(rule);
        true
    }

    pub fn set_profile(&mut self, program: &str, profile: &str) -> bool {
        let name = program_name(program);
        if profile.is_empty() {
            return false;
        }
        match self.0.iter_mut().find(|rule| rule.program == name) {
            Some(rule) => {
                rule.profile = profile.to_owned();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, program: &str) -> bool {
        let name = program_name(program);
        let before = self.0.len();
        self.0.retain(|rule| rule.program != name);
        self.0.len() != before
    }

    pub fn remove_profile(&mut self, profile: &str) -> usize {
        let before = self.0.len();
        self.0.retain(|rule| rule.profile != profile);
        before - self.0.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front<'a> {
    Program(&'a str),
    Own,
    Nothing,
}

pub trait Surroundings {
    fn profile_exists(&self, profile: &str) -> bool;

    fn profile_sets_output_type(&self, profile: &str) -> bool;

    fn is_running(&self, program: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleSelector {
    applying: Option<String>,
}

impl RuleSelector {
    pub fn choose<'r>(
        &mut self,
        rules: &'r GameRules,
        surroundings: &dyn Surroundings,
        front: Front<'_>,
    ) -> Option<&'r GameRule> {
        let usable = |rule: &&GameRule| surroundings.profile_exists(&rule.profile);
        match front {
            Front::Own => {}
            Front::Program(program) => match rules.find(program).filter(usable) {
                Some(rule) => self.applying = Some(rule.program.clone()),
                None => self.leave(rules, surroundings),
            },
            Front::Nothing => self.leave(rules, surroundings),
        }
        let applying = self.applying.as_deref()?;
        let rule = rules.find(applying).filter(usable);
        if rule.is_none() {
            self.applying = None;
        }
        rule
    }

    fn leave(&mut self, rules: &GameRules, surroundings: &dyn Surroundings) {
        let Some(applying) = &self.applying else {
            return;
        };
        let stays = rules.find(applying).is_some_and(|rule| {
            surroundings.profile_exists(&rule.profile)
                && surroundings.profile_sets_output_type(&rule.profile)
                && surroundings.is_running(&rule.program)
        });
        if !stays {
            self.applying = None;
        }
    }

    pub fn applying(&self) -> Option<&str> {
        self.applying.as_deref()
    }
}
