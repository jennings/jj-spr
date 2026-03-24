/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[derive(Clone, Debug)]
pub struct Error {
    messages: Vec<String>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new<S>(message: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            messages: vec![message.into()],
        }
    }

    pub fn empty() -> Self {
        Self {
            messages: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn messages(&self) -> &Vec<String> {
        &self.messages
    }

    pub fn push(&mut self, message: String) {
        self.messages.push(message);
    }
}

impl<E> From<E> for Error
where
    E: std::error::Error,
{
    fn from(error: E) -> Self {
        // Walk the error source chain so that wrapped errors (e.g.
        // octocrab::Error::GitHub whose Display is just "GitHub") include
        // the underlying message.
        let mut msg = format!("{}", error);
        let mut source = error.source();
        while let Some(s) = source {
            msg = format!("{}: {}", msg, s);
            source = s.source();
        }
        Self {
            messages: vec![msg],
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = self.messages.last();
        if let Some(message) = message {
            write!(f, "{}", message)
        } else {
            write!(f, "unknown error")
        }
    }
}

pub trait ResultExt {
    type Output;

    fn convert(self) -> Self::Output;
    fn context(self, message: String) -> Self::Output;
    fn reword(self, message: String) -> Self::Output;
}
impl<T> ResultExt for Result<T> {
    type Output = Self;

    fn convert(self) -> Self {
        self
    }

    fn context(mut self, message: String) -> Self {
        if let Err(error) = &mut self {
            error.push(message);
        }

        self
    }

    fn reword(mut self, message: String) -> Self {
        if let Err(error) = &mut self {
            error.messages.pop();
            error.push(message);
        }

        self
    }
}

impl<T, E> ResultExt for std::result::Result<T, E>
where
    E: std::error::Error,
{
    type Output = Result<T>;

    fn convert(self) -> Result<T> {
        match self {
            Ok(v) => Ok(v),
            Err(error) => Err(error.into()),
        }
    }

    fn context(self, message: String) -> Result<T> {
        self.convert().context(message)
    }

    fn reword(self, message: String) -> Result<T> {
        self.convert().reword(message)
    }
}

pub struct Terminator {
    error: Error,
}

impl From<Error> for Terminator {
    fn from(error: Error) -> Self {
        Self { error }
    }
}

impl std::fmt::Debug for Terminator {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "🛑 ")?;
        for message in self.error.messages.iter().rev() {
            writeln!(f, "{}", message)?;
        }
        Ok(())
    }
}

impl<E> From<E> for Terminator
where
    E: std::error::Error,
{
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
        }
    }
}

pub fn add_error<T, U>(result: &mut Result<T>, other: Result<U>) -> Option<U> {
    match other {
        Ok(result) => Some(result),
        Err(error) => {
            if let Err(e) = result {
                e.messages.extend(error.messages);
            } else {
                *result = Err(error);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct InnerError(String);

    impl std::fmt::Display for InnerError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for InnerError {}

    #[derive(Debug)]
    struct OuterError {
        msg: String,
        source: InnerError,
    }

    impl std::fmt::Display for OuterError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.msg)
        }
    }

    impl std::error::Error for OuterError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn test_from_error_includes_source_chain() {
        let outer = OuterError {
            msg: "GitHub".into(),
            source: InnerError("PR is not mergeable".into()),
        };
        let error: Error = outer.into();
        assert_eq!(error.messages()[0], "GitHub: PR is not mergeable");
    }

    #[test]
    fn test_from_error_without_source() {
        let inner = InnerError("simple error".into());
        let error: Error = inner.into();
        assert_eq!(error.messages()[0], "simple error");
    }

    #[test]
    fn test_context_appends_message() {
        let result: Result<()> = Err(Error::new("original"));
        let result = result.context("added context".into());
        let err = result.unwrap_err();
        assert_eq!(err.messages(), &["original", "added context"]);
    }

    #[test]
    fn test_reword_replaces_last_message() {
        let result: Result<()> = Err(Error::new("original"));
        let result = result.reword("reworded".into());
        let err = result.unwrap_err();
        assert_eq!(err.messages(), &["reworded"]);
    }
}
