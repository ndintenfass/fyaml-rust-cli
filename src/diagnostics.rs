use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    InvalidInput,
    Parse,
    Write,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub paths: Vec<String>,
    pub derived_key_path: Option<String>,
    pub location: Option<String>,
    pub cause: String,
    pub action: String,
    pub context: Option<String>,
    #[serde(skip_serializing)]
    pub category: Category,
}

impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
        category: Category,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            paths: Vec::new(),
            derived_key_path: None,
            location: None,
            cause: String::new(),
            action: String::new(),
            context: None,
            category,
        }
    }

    pub fn with_paths(mut self, paths: Vec<String>) -> Self {
        self.paths = paths;
        self
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn with_derived_key_path(mut self, derived_key_path: impl Into<String>) -> Self {
        self.derived_key_path = Some(derived_key_path.into());
        self
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = cause.into();
        self
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>, category: Category) -> Self {
        Self::new(code, Severity::Error, message, category)
    }

    pub fn warn(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warn, message, Category::InvalidInput)
    }

    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Info, message, Category::Internal)
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn is_warning(&self) -> bool {
        self.severity == Severity::Warn
    }

    pub fn render_human(&self) -> String {
        let mut out = String::new();
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warn => "warn",
            Severity::Info => "info",
        };

        out.push_str(&format!("{sev}[{}]: {}\n", self.code, self.message));

        if let Some(location) = &self.location {
            out.push_str(&format!("  Location: {location}\n"));
        } else if !self.paths.is_empty() {
            out.push_str(&format!("  Location: {}\n", self.paths.join(", ")));
        }

        if !self.cause.is_empty() {
            out.push_str(&format!("  Cause: {}\n", self.cause));
        }

        if !self.action.is_empty() {
            out.push_str(&format!("  Action: {}\n", self.action));
        }

        if let Some(context) = &self.context {
            out.push_str(&format!("  Context: {}\n", context));
        }

        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    Internal = 1,
    InvalidInput = 2,
    ParseError = 3,
    WriteError = 5,
}

impl ExitCode {
    pub fn from_diagnostics(diags: &[Diagnostic]) -> Self {
        let has_parse = diags
            .iter()
            .any(|d| d.is_error() && d.category == Category::Parse);
        if has_parse {
            return ExitCode::ParseError;
        }

        let has_write = diags
            .iter()
            .any(|d| d.is_error() && d.category == Category::Write);
        if has_write {
            return ExitCode::WriteError;
        }

        let has_input = diags
            .iter()
            .any(|d| d.is_error() && d.category == Category::InvalidInput);
        if has_input {
            return ExitCode::InvalidInput;
        }

        let has_internal = diags
            .iter()
            .any(|d| d.is_error() && d.category == Category::Internal);
        if has_internal {
            return ExitCode::Internal;
        }

        ExitCode::Success
    }
}
