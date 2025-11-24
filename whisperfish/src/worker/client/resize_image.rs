#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentQuality {
    High,
    Standard,
    Low,
}

impl AttachmentQuality {
    pub const fn as_str(&self) -> &'static str {
        match self {
            AttachmentQuality::High => "high",
            AttachmentQuality::Standard => "standard",
            AttachmentQuality::Low => "low",
        }
    }
}

impl From<&str> for AttachmentQuality {
    fn from(value: &str) -> Self {
        match value {
            "high" => AttachmentQuality::High,
            "standard" => AttachmentQuality::Standard,
            "low" => AttachmentQuality::Low,
            x => {
                tracing::warn!("Unknown AttachmentQuality value {x}, returning standard");
                AttachmentQuality::Standard
            }
        }
    }
}

impl From<AttachmentQuality> for String {
    fn from(quality: AttachmentQuality) -> Self {
        quality.as_str().to_string()
    }
}

impl AsRef<str> for AttachmentQuality {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

