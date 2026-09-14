use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

static IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<base>(?:\d{4}\.\d{4,5}|[a-zA-Z][a-zA-Z.-]*(?:\.[A-Z]{2})?/\d{7}))(?P<version>v[1-9]\d*)?$")
        .expect("valid identifier expression")
});

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Identifier {
    pub base: String,
    pub version: Option<String>,
}

impl Identifier {
    pub fn cache_key(&self) -> String {
        self.to_string().replace('/', "_")
    }
}

impl FromStr for Identifier {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let mut value = value.trim();
        for prefix in [
            "https://arxiv.org/abs/",
            "http://arxiv.org/abs/",
            "https://arxiv.org/pdf/",
            "https://arxiv.org/html/",
            "arXiv:",
            "arxiv:",
        ] {
            if let Some(identifier) = value.strip_prefix(prefix) {
                value = identifier;
                break;
            }
        }
        let value = value.trim_end_matches('/').trim_end_matches(".pdf");
        let Some(captures) = IDENTIFIER.captures(value) else {
            bail!("invalid arXiv identifier: {value:?}; expected 1706.03762 or hep-th/9901001, optionally followed by vN");
        };
        Ok(Self {
            base: captures["base"].to_owned(),
            version: captures.name("version").map(|version| version.as_str().to_owned()),
        })
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.base, self.version.as_deref().unwrap_or(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_modern_legacy_and_url_identifiers() {
        for value in ["1706.03762v7", "hep-th/9901001v2", "math.GT/0309136"] {
            assert_eq!(value.parse::<Identifier>().unwrap().to_string(), value);
        }
        assert_eq!(
            "https://arxiv.org/pdf/1706.03762v1.pdf"
                .parse::<Identifier>()
                .unwrap()
                .version
                .as_deref(),
            Some("v1")
        );
    }

    #[test]
    fn rejects_path_traversal_and_invalid_versions() {
        for value in ["../secret", "1706.03762v0", "1706.03762/../../x", "x"] {
            assert!(value.parse::<Identifier>().is_err());
        }
    }
}
