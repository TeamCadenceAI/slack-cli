//! File model for Slack API

use serde::{Deserialize, Serialize};

/// A Slack file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct File {
    /// File ID
    pub id: String,

    /// Unix timestamp when file was created
    #[serde(default)]
    pub created: Option<i64>,

    /// Unix timestamp when file was last updated
    #[serde(default)]
    pub timestamp: Option<i64>,

    /// File name
    #[serde(default)]
    pub name: Option<String>,

    /// File title
    #[serde(default)]
    pub title: Option<String>,

    /// MIME type
    #[serde(default)]
    pub mimetype: Option<String>,

    /// File type (e.g., "png", "pdf")
    #[serde(default)]
    pub filetype: Option<String>,

    /// Pretty file type for display
    #[serde(default)]
    pub pretty_type: Option<String>,

    /// User ID who uploaded the file
    #[serde(default)]
    pub user: Option<String>,

    /// Team ID where file was uploaded
    #[serde(default)]
    pub user_team: Option<String>,

    /// Whether the file can be edited
    #[serde(default)]
    pub editable: bool,

    /// File size in bytes
    #[serde(default)]
    pub size: Option<u64>,

    /// File mode (e.g., "hosted", "external")
    #[serde(default)]
    pub mode: Option<String>,

    /// Whether the file is external
    #[serde(default)]
    pub is_external: bool,

    /// External type (if external)
    #[serde(default)]
    pub external_type: Option<String>,

    /// Whether the file is public
    #[serde(default)]
    pub is_public: bool,

    /// Whether public URL is shared
    #[serde(default)]
    pub public_url_shared: bool,

    /// Whether the file can be displayed inline
    #[serde(default)]
    pub display_as_bot: bool,

    /// Username of uploader
    #[serde(default)]
    pub username: Option<String>,

    // URLs
    /// URL for private access (requires auth)
    #[serde(default)]
    pub url_private: Option<String>,

    /// URL for private download (requires auth)
    #[serde(default)]
    pub url_private_download: Option<String>,

    /// Permalink to file
    #[serde(default)]
    pub permalink: Option<String>,

    /// Public permalink (if public)
    #[serde(default)]
    pub permalink_public: Option<String>,

    /// Edit link (if editable)
    #[serde(default)]
    pub edit_link: Option<String>,

    /// Preview content (for text files)
    #[serde(default)]
    pub preview: Option<String>,

    /// Preview highlighted (for code files)
    #[serde(default)]
    pub preview_highlight: Option<String>,

    /// Number of lines (for text files)
    #[serde(default)]
    pub lines: Option<u32>,

    /// Number of lines more (truncated preview)
    #[serde(default)]
    pub lines_more: Option<u32>,

    /// Preview truncated flag
    #[serde(default)]
    pub preview_is_truncated: bool,

    // Image-specific fields
    /// Original width (images)
    #[serde(default)]
    pub original_w: Option<u32>,

    /// Original height (images)
    #[serde(default)]
    pub original_h: Option<u32>,

    /// Thumbnail URLs at various sizes
    #[serde(default)]
    pub thumb_64: Option<String>,
    #[serde(default)]
    pub thumb_80: Option<String>,
    #[serde(default)]
    pub thumb_160: Option<String>,
    #[serde(default)]
    pub thumb_360: Option<String>,
    #[serde(default)]
    pub thumb_360_w: Option<u32>,
    #[serde(default)]
    pub thumb_360_h: Option<u32>,
    #[serde(default)]
    pub thumb_480: Option<String>,
    #[serde(default)]
    pub thumb_480_w: Option<u32>,
    #[serde(default)]
    pub thumb_480_h: Option<u32>,
    #[serde(default)]
    pub thumb_720: Option<String>,
    #[serde(default)]
    pub thumb_720_w: Option<u32>,
    #[serde(default)]
    pub thumb_720_h: Option<u32>,
    #[serde(default)]
    pub thumb_800: Option<String>,
    #[serde(default)]
    pub thumb_800_w: Option<u32>,
    #[serde(default)]
    pub thumb_800_h: Option<u32>,
    #[serde(default)]
    pub thumb_960: Option<String>,
    #[serde(default)]
    pub thumb_960_w: Option<u32>,
    #[serde(default)]
    pub thumb_960_h: Option<u32>,
    #[serde(default)]
    pub thumb_1024: Option<String>,
    #[serde(default)]
    pub thumb_1024_w: Option<u32>,
    #[serde(default)]
    pub thumb_1024_h: Option<u32>,

    // Video-specific fields
    /// Duration in milliseconds (video)
    #[serde(default)]
    pub duration_ms: Option<u64>,

    // Sharing info
    /// Channels file is shared to
    #[serde(default)]
    pub channels: Option<Vec<String>>,

    /// Groups file is shared to
    #[serde(default)]
    pub groups: Option<Vec<String>>,

    /// IMs file is shared to
    #[serde(default)]
    pub ims: Option<Vec<String>>,

    /// Number of comments
    #[serde(default)]
    pub comments_count: Option<u32>,

    /// Whether file has been starred
    #[serde(default)]
    pub is_starred: bool,

    /// Pinned info
    #[serde(default)]
    pub pinned_to: Option<Vec<String>>,
}

impl File {
    /// Get the download URL for this file
    pub fn download_url(&self) -> Option<&str> {
        self.url_private_download
            .as_deref()
            .or(self.url_private.as_deref())
    }

    /// Check if this is an image file
    pub fn is_image(&self) -> bool {
        self.mimetype
            .as_ref()
            .is_some_and(|m| m.starts_with("image/"))
    }

    /// Check if this is a video file
    pub fn is_video(&self) -> bool {
        self.mimetype
            .as_ref()
            .is_some_and(|m| m.starts_with("video/"))
    }

    /// Check if this is a text file
    pub fn is_text(&self) -> bool {
        self.mimetype.as_ref().is_some_and(|m| {
            m.starts_with("text/") || m == "application/json" || m == "application/xml"
        })
    }

    /// Get the file size in a human-readable format
    pub fn human_size(&self) -> String {
        match self.size {
            Some(size) => {
                if size < 1024 {
                    format!("{} B", size)
                } else if size < 1024 * 1024 {
                    format!("{:.1} KB", size as f64 / 1024.0)
                } else if size < 1024 * 1024 * 1024 {
                    format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                } else {
                    format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
                }
            }
            None => "unknown".to_string(),
        }
    }

    /// Check if the file is within the size limit for download (5MB)
    pub fn is_within_download_limit(&self) -> bool {
        self.size.map_or(true, |s| s <= 5 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_deserialization_image() {
        let json = r#"{
            "id": "F1234567890",
            "created": 1577836800,
            "timestamp": 1577836800,
            "name": "image.png",
            "title": "My Image",
            "mimetype": "image/png",
            "filetype": "png",
            "pretty_type": "PNG",
            "user": "U1234567890",
            "size": 102400,
            "mode": "hosted",
            "is_external": false,
            "is_public": false,
            "url_private": "https://files.slack.com/files-pri/T1234-F1234/image.png",
            "url_private_download": "https://files.slack.com/files-pri/T1234-F1234/download/image.png",
            "permalink": "https://myteam.slack.com/files/U1234/F1234/image.png",
            "original_w": 800,
            "original_h": 600,
            "thumb_64": "https://files.slack.com/files-tmb/T1234-F1234-64/image.png",
            "thumb_80": "https://files.slack.com/files-tmb/T1234-F1234-80/image.png"
        }"#;

        let file: File = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "F1234567890");
        assert_eq!(file.name, Some("image.png".to_string()));
        assert_eq!(file.mimetype, Some("image/png".to_string()));
        assert!(file.is_image());
        assert!(!file.is_video());
        assert_eq!(file.original_w, Some(800));
        assert_eq!(file.original_h, Some(600));
    }

    #[test]
    fn test_file_deserialization_text() {
        let json = r#"{
            "id": "F1234567890",
            "name": "code.py",
            "mimetype": "text/plain",
            "filetype": "python",
            "size": 1024,
            "preview": "import os\n\ndef main():\n    print('Hello')",
            "lines": 10,
            "lines_more": 5,
            "preview_is_truncated": true
        }"#;

        let file: File = serde_json::from_str(json).unwrap();
        assert!(file.is_text());
        assert!(file.preview.is_some());
        assert_eq!(file.lines, Some(10));
        assert!(file.preview_is_truncated);
    }

    #[test]
    fn test_file_deserialization_video() {
        let json = r#"{
            "id": "F1234567890",
            "name": "video.mp4",
            "mimetype": "video/mp4",
            "filetype": "mp4",
            "size": 10485760,
            "duration_ms": 60000
        }"#;

        let file: File = serde_json::from_str(json).unwrap();
        assert!(file.is_video());
        assert!(!file.is_image());
        assert_eq!(file.duration_ms, Some(60000));
    }

    #[test]
    fn test_file_deserialization_with_sharing() {
        let json = r#"{
            "id": "F1234567890",
            "name": "doc.pdf",
            "channels": ["C123", "C456"],
            "groups": ["G123"],
            "ims": [],
            "comments_count": 5,
            "is_starred": true,
            "pinned_to": ["C123"]
        }"#;

        let file: File = serde_json::from_str(json).unwrap();
        assert_eq!(
            file.channels,
            Some(vec!["C123".to_string(), "C456".to_string()])
        );
        assert_eq!(file.comments_count, Some(5));
        assert!(file.is_starred);
    }

    #[test]
    fn test_file_download_url() {
        let file_with_download = File {
            id: "F123".to_string(),
            url_private: Some("https://example.com/private".to_string()),
            url_private_download: Some("https://example.com/download".to_string()),
            ..Default::default()
        };
        assert_eq!(
            file_with_download.download_url(),
            Some("https://example.com/download")
        );

        let file_without_download = File {
            id: "F123".to_string(),
            url_private: Some("https://example.com/private".to_string()),
            url_private_download: None,
            ..Default::default()
        };
        assert_eq!(
            file_without_download.download_url(),
            Some("https://example.com/private")
        );

        let file_no_url = File {
            id: "F123".to_string(),
            ..Default::default()
        };
        assert!(file_no_url.download_url().is_none());
    }

    #[test]
    fn test_file_human_size() {
        let bytes = File {
            id: "F123".to_string(),
            size: Some(500),
            ..Default::default()
        };
        assert_eq!(bytes.human_size(), "500 B");

        let kb = File {
            id: "F123".to_string(),
            size: Some(2048),
            ..Default::default()
        };
        assert_eq!(kb.human_size(), "2.0 KB");

        let mb = File {
            id: "F123".to_string(),
            size: Some(5 * 1024 * 1024),
            ..Default::default()
        };
        assert_eq!(mb.human_size(), "5.0 MB");

        let gb = File {
            id: "F123".to_string(),
            size: Some(2 * 1024 * 1024 * 1024),
            ..Default::default()
        };
        assert_eq!(gb.human_size(), "2.0 GB");

        let unknown = File {
            id: "F123".to_string(),
            size: None,
            ..Default::default()
        };
        assert_eq!(unknown.human_size(), "unknown");
    }

    #[test]
    fn test_file_within_download_limit() {
        let small = File {
            id: "F123".to_string(),
            size: Some(1024 * 1024), // 1MB
            ..Default::default()
        };
        assert!(small.is_within_download_limit());

        let at_limit = File {
            id: "F123".to_string(),
            size: Some(5 * 1024 * 1024), // 5MB
            ..Default::default()
        };
        assert!(at_limit.is_within_download_limit());

        let over_limit = File {
            id: "F123".to_string(),
            size: Some(6 * 1024 * 1024), // 6MB
            ..Default::default()
        };
        assert!(!over_limit.is_within_download_limit());

        let unknown_size = File {
            id: "F123".to_string(),
            size: None,
            ..Default::default()
        };
        assert!(unknown_size.is_within_download_limit());
    }

    #[test]
    fn test_file_minimal() {
        let json = r#"{"id": "F123"}"#;
        let file: File = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "F123");
        assert!(file.name.is_none());
    }
}
