// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Media content extraction for Telegram messages.
//!
//! Downloads files from Telegram servers and converts them to
//! [`MessageContent`] variants for the channel adapter.

use blufio_core::error::BlufioError;
use blufio_core::types::MessageContent;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{Document, FileMeta, PhotoSize, Voice};
use tracing::debug;

/// Downloads a file from Telegram servers by its file metadata.
///
/// Uses the Bot API's `getFile` to resolve the file path, then downloads
/// the file content as bytes.
pub async fn download_file(bot: &Bot, file_meta: &FileMeta) -> Result<Vec<u8>, BlufioError> {
    let file = bot
        .get_file(file_meta.id.clone())
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to get file info: {e}"),
            source: Some(Box::new(e)),
        })?;

    let mut buf = Vec::new();
    bot.download_file(&file.path, &mut buf)
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to download file: {e}"),
            source: Some(Box::new(e)),
        })?;

    debug!(
        file_id = %file_meta.id,
        size = buf.len(),
        "downloaded file from Telegram"
    );
    Ok(buf)
}

/// Extracts image content from a Telegram photo message.
///
/// Downloads the largest available photo variant (last in the array).
/// Returns [`MessageContent::Image`] with JPEG mime type.
pub async fn extract_photo_content(
    bot: &Bot,
    photos: &[PhotoSize],
    caption: Option<&str>,
) -> Result<MessageContent, BlufioError> {
    // Telegram provides multiple sizes; the last one is the largest.
    let largest = photos.last().ok_or_else(|| BlufioError::Channel {
        message: "photo array is empty".into(),
        source: None,
    })?;

    let data = download_file(bot, &largest.file).await?;

    Ok(MessageContent::Image {
        data,
        mime_type: "image/jpeg".to_string(),
        caption: caption.map(|s| s.to_string()),
    })
}

/// Extracts document content from a Telegram document message.
///
/// Downloads the document file and determines the filename and MIME type
/// from the Telegram metadata.
pub async fn extract_document_content(
    bot: &Bot,
    doc: &Document,
) -> Result<MessageContent, BlufioError> {
    let data = download_file(bot, &doc.file).await?;

    let filename = doc
        .file_name
        .clone()
        .unwrap_or_else(|| "document".to_string());

    let mime_type = doc
        .mime_type
        .as_ref()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok(MessageContent::Document {
        data,
        filename,
        mime_type,
    })
}

/// Extracts voice content from a Telegram voice message.
///
/// Downloads the voice file (typically OGG format) and captures the duration.
pub async fn extract_voice_content(
    bot: &Bot,
    voice: &Voice,
) -> Result<MessageContent, BlufioError> {
    let data = download_file(bot, &voice.file).await?;

    // voice.duration is teloxide's Seconds type -- convert to f32
    let duration_secs = Some(voice.duration.seconds() as f32);

    Ok(MessageContent::Voice {
        data,
        duration_secs,
    })
}
