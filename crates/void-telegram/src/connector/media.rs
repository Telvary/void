use std::path::Path;

use grammers_client::client::Client;
use grammers_client::media::Downloadable;
use grammers_client::message::InputMessage;

use crate::error::TelegramError;

/// Decide whether a file should be sent as a Telegram photo (vs. a generic
/// document) based on its MIME type and/or file extension.
pub(crate) fn is_image_media(path: &Path, mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(|m| m.starts_with("image/"))
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp"))
}

pub(crate) async fn upload_and_build_media_message(
    client: &Client,
    path: &Path,
    caption: Option<&str>,
    mime_type: Option<&str>,
) -> anyhow::Result<InputMessage> {
    let uploaded = client.upload_file(path).await?;
    let caption_text = caption.unwrap_or("");

    let is_image = is_image_media(path, mime_type);

    let msg = if is_image {
        InputMessage::new().text(caption_text).photo(uploaded)
    } else {
        InputMessage::new().text(caption_text).document(uploaded)
    };

    Ok(msg)
}

pub(crate) async fn download_media_to_bytes<D: Downloadable>(
    client: &Client,
    downloadable: &D,
) -> Result<Vec<u8>, TelegramError> {
    let mut bytes = Vec::new();
    let mut download = client.iter_download(downloadable);
    while let Some(chunk) = download
        .next()
        .await
        .map_err(|e| TelegramError::Media(e.to_string()))?
    {
        bytes.extend(chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::is_image_media;
    use std::path::Path;

    #[test]
    fn image_by_mime_type() {
        assert!(is_image_media(Path::new("file.bin"), Some("image/png")));
        assert!(is_image_media(Path::new("noext"), Some("image/jpeg")));
    }

    #[test]
    fn non_image_mime_type() {
        assert!(!is_image_media(
            Path::new("file.bin"),
            Some("application/pdf")
        ));
        assert!(!is_image_media(Path::new("clip"), Some("video/mp4")));
    }

    #[test]
    fn image_by_extension_when_mime_absent() {
        for name in ["a.jpg", "a.jpeg", "a.png", "a.gif", "a.webp"] {
            assert!(
                is_image_media(Path::new(name), None),
                "{name} should be image"
            );
        }
    }

    #[test]
    fn image_extension_is_case_sensitive() {
        // The matcher only accepts lowercase extensions.
        assert!(!is_image_media(Path::new("PHOTO.JPG"), None));
    }

    #[test]
    fn non_image_extension_when_mime_absent() {
        assert!(!is_image_media(Path::new("doc.pdf"), None));
        assert!(!is_image_media(Path::new("clip.mp4"), None));
        assert!(!is_image_media(Path::new("noextension"), None));
    }

    #[test]
    fn mime_type_takes_priority_over_extension() {
        // image MIME wins even with a non-image extension.
        assert!(is_image_media(Path::new("report.pdf"), Some("image/png")));
        // non-image MIME does not override an image extension (falls through to ext check).
        assert!(is_image_media(Path::new("photo.png"), Some("text/plain")));
    }
}
