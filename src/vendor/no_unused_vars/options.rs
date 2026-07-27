// Synced from oxc @ 14533a3dc118bea73e755426aaf35f71dbe81eb8: crates/oxc_linter/src/rules/eslint/no_unused_vars/options.rs

use std::ops::Deref;

use lazy_regex::{Regex, RegexBuilder};

#[derive(Debug, Clone)]
#[must_use]
#[non_exhaustive]
pub struct NoUnusedVarsOptions {
    pub vars: VarsOption,
    pub vars_ignore_pattern: IgnorePattern<Regex>,
    pub args: ArgsOption,
    pub args_ignore_pattern: IgnorePattern<Regex>,
    pub ignore_rest_siblings: bool,
    pub caught_errors: CaughtErrors,
    pub caught_errors_ignore_pattern: IgnorePattern<Regex>,
    pub destructured_array_ignore_pattern: IgnorePattern<Regex>,
    pub ignore_class_with_static_init_block: bool,
    pub ignore_using_declarations: bool,
    pub report_used_ignore_pattern: bool,
    pub report_vars_only_used_as_types: bool,
}

impl Default for NoUnusedVarsOptions {
    fn default() -> Self {
        Self {
            vars: VarsOption::default(),
            vars_ignore_pattern: IgnorePattern::Default,
            args: ArgsOption::default(),
            args_ignore_pattern: IgnorePattern::Default,
            ignore_rest_siblings: false,
            caught_errors: CaughtErrors::default(),
            caught_errors_ignore_pattern: IgnorePattern::None,
            destructured_array_ignore_pattern: IgnorePattern::None,
            ignore_class_with_static_init_block: false,
            ignore_using_declarations: false,
            report_used_ignore_pattern: false,
            report_vars_only_used_as_types: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum IgnorePattern<R> {
    Default,
    None,
    Some(R),
}

impl<R> IgnorePattern<R> {
    #[inline]
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_) | Self::Default)
    }

    #[inline]
    pub fn as_ref(&self) -> IgnorePattern<&R> {
        match self {
            Self::Default => IgnorePattern::Default,
            Self::None => IgnorePattern::None,
            Self::Some(pattern) => IgnorePattern::Some(pattern),
        }
    }
}

impl TryFrom<Option<&str>> for IgnorePattern<Regex> {
    type Error = lazy_regex::regex::Error;

    fn try_from(value: Option<&str>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Self::None),
            Some("^_") => Ok(Self::Default),
            Some(pattern) => RegexBuilder::new(pattern).build().map(Self::Some),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum VarsOption {
    #[default]
    All,
    Local,
}

impl VarsOption {
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ArgsOption {
    #[default]
    AfterUsed,
    All,
    None,
}

impl ArgsOption {
    #[inline]
    pub const fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    #[inline]
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct CaughtErrors(bool);

impl Default for CaughtErrors {
    fn default() -> Self {
        Self::all()
    }
}

impl CaughtErrors {
    pub const fn all() -> Self {
        Self(true)
    }

    pub const fn none() -> Self {
        Self(false)
    }
}

impl Deref for CaughtErrors {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::Not for CaughtErrors {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl From<bool> for CaughtErrors {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
