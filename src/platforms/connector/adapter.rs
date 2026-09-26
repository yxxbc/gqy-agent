//! 连接器平台的 `PlatformAdapter`：把平台中立的出站消息拆成 `send` 帧，
//! 一帧一个气泡或一个附件，逐帧等连接器回执。
//!
//! 拆气泡按段落（空行）来，段数超过 `max_bubbles` 时均衡合并——让最长的那个
//! 气泡尽量短，而不是前面几个很短、最后一个塞满（和旧桥接的算法一致）。气泡
//! 之间按字数停顿一会儿，像在打字。

use super::protocol::{SendPart, ServerFrame, MAX_ATTACHMENT_BYTES};
use super::registry::{ConnectorHandle, ATTACHMENT_SEND_TIMEOUT, TEXT_SEND_TIMEOUT};
use crate::config::ConnectorPlatformConfig;
use crate::platforms::*;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use futures_util::future::BoxFuture;
use std::path::Path;

pub(crate) struct ConnectorAdapter {
    handle: ConnectorHandle,
    /// 连接器侧的收件人（回给发来消息的那个账号）。
    to: String,
    max_bubbles: usize,
    pause_ceiling: Duration,
}

impl ConnectorAdapter {
    pub(crate) fn new(
        handle: ConnectorHandle,
        to: String,
        settings: &ConnectorPlatformConfig,
    ) -> Self {
        Self {
            handle,
            to,
            max_bubbles: settings.max_bubbles.max(1),
            pause_ceiling: Duration::from_secs_f64(settings.bubble_pause_seconds.max(0.0)),
        }
    }

    async fn send_segments(&self, segments: Vec<OutboundSegment>) -> Result<SendReceipt> {
        let parts = self.plan(segments).await?;
        let mut receipt = SendReceipt::default();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                tokio::time::sleep(bubble_pause(part.weight(), self.pause_ceiling)).await;
            }
            match self.send_part(part).await {
                Ok(message_id) => {
                    receipt.delivered_parts += 1;
                    if let Some(message_id) = message_id {
                        receipt.message_ids.push(message_id);
                    }
                    if let Planned::Image { data, .. } = part {
                        receipt.image_digests.push(blake3::hash(data));
                    }
                }
                Err(error) => {
                    return Err(if receipt.has_delivery() {
                        anyhow::Error::new(PartialSendError::new(error, receipt))
                    } else {
                        error
                    });
                }
            }
        }
        Ok(receipt)
    }

    /// 出站段落 → 要发的帧。相邻的文字并在一起再拆气泡。
    async fn plan(&self, segments: Vec<OutboundSegment>) -> Result<Vec<Planned>> {
        let mut planned = Vec::new();
        let mut text = String::new();
        let flush = |text: &mut String, planned: &mut Vec<Planned>, max_bubbles: usize| {
            for bubble in split_bubbles(text, max_bubbles) {
                planned.push(Planned::Text(bubble));
            }
            text.clear();
        };
        for segment in segments {
            match segment {
                OutboundSegment::Markdown(markdown) => {
                    push_paragraph(&mut text, &markdown_to_plain(&markdown))
                }
                OutboundSegment::Text(plain) => push_paragraph(&mut text, &plain),
                OutboundSegment::Mention(_) => {}
                OutboundSegment::ImageBytes { mime, data, .. } => {
                    flush(&mut text, &mut planned, self.max_bubbles);
                    planned.push(Planned::Image {
                        mime,
                        data: data.to_vec(),
                    });
                }
                OutboundSegment::ImagePath { path, .. } => {
                    flush(&mut text, &mut planned, self.max_bubbles);
                    let data = read_capped(&path).await?;
                    let mime = sniff_image_mime(&data).to_string();
                    planned.push(Planned::Image { mime, data });
                }
                OutboundSegment::FilePath { path, name } => {
                    flush(&mut text, &mut planned, self.max_bubbles);
                    let data = read_capped(&path).await?;
                    let name = name.unwrap_or_else(|| file_name(&path));
                    planned.push(Planned::File { name, data });
                }
                OutboundSegment::AudioPath { path, .. } => {
                    flush(&mut text, &mut planned, self.max_bubbles);
                    let data = read_capped(&path).await?;
                    planned.push(Planned::Audio {
                        name: file_name(&path),
                        data,
                    });
                }
            }
        }
        flush(&mut text, &mut planned, self.max_bubbles);
        self.check_capabilities(&planned)?;
        Ok(planned)
    }

    fn check_capabilities(&self, planned: &[Planned]) -> Result<()> {
        let capabilities = &self.handle.capabilities;
        for part in planned {
            let missing = match part {
                Planned::Text(_) => None,
                Planned::Image { .. } => (!capabilities.image_out).then_some("images"),
                Planned::Audio { .. } => (!capabilities.audio_out).then_some("voice messages"),
                Planned::File { .. } => (!capabilities.file_out).then_some("files"),
            };
            if let Some(missing) = missing {
                bail!(
                    "the {} connector cannot send {missing}",
                    self.handle.display_name
                );
            }
        }
        Ok(())
    }

    async fn send_part(&self, part: &Planned) -> Result<Option<String>> {
        let to = self.to.as_str();
        let timeout = match part {
            Planned::Text(_) => TEXT_SEND_TIMEOUT,
            _ => ATTACHMENT_SEND_TIMEOUT,
        };
        let result = self
            .handle
            .request(
                |req| {
                    let part = match part {
                        Planned::Text(text) => SendPart::Text { text },
                        Planned::Image { mime, data } => SendPart::Image {
                            mime,
                            name: image_name(mime),
                            data: BASE64.encode(data),
                        },
                        Planned::Audio { name, data } => SendPart::Audio {
                            mime: audio_mime(name),
                            name,
                            data: BASE64.encode(data),
                        },
                        Planned::File { name, data } => SendPart::File {
                            mime: "application/octet-stream",
                            name,
                            data: BASE64.encode(data),
                        },
                    };
                    ServerFrame::Send { req, to, part }.encode()
                },
                timeout,
            )
            .await?;
        if result.ok {
            Ok(result.message_id)
        } else {
            bail!(
                "the {} connector failed to send: {}",
                self.handle.display_name,
                result.error.as_deref().unwrap_or("unknown error")
            )
        }
    }
}

impl PlatformAdapter for ConnectorAdapter {
    fn send<'a>(&'a self, message: OutboundMessage) -> BoxFuture<'a, Result<SendReceipt>> {
        Box::pin(async move {
            let segments = match message.body {
                OutboundBody::Segments(segments) => segments,
                // 合并转发在聊天软件里没有对应物：摊平成「谁：说了什么」。
                OutboundBody::Forward(nodes) => nodes
                    .into_iter()
                    .flat_map(|node| {
                        let name = node.display_name;
                        node.segments.into_iter().map(move |segment| match segment {
                            OutboundSegment::Markdown(text) | OutboundSegment::Text(text) => {
                                OutboundSegment::Text(format!(
                                    "{name}: {}",
                                    markdown_to_plain(&text)
                                ))
                            }
                            other => other,
                        })
                    })
                    .collect(),
            };
            self.send_segments(segments).await
        })
    }

    fn bot_display_name<'a>(&'a self) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(self.handle.display_name.clone()) })
    }
}

enum Planned {
    Text(String),
    Image { mime: String, data: Vec<u8> },
    Audio { name: String, data: Vec<u8> },
    File { name: String, data: Vec<u8> },
}

impl Planned {
    /// 停顿按它算：文字按字数，附件按一个短句。
    fn weight(&self) -> usize {
        match self {
            Planned::Text(text) => text.chars().count(),
            _ => 20,
        }
    }
}

fn push_paragraph(text: &mut String, paragraph: &str) {
    let paragraph = paragraph.trim();
    if paragraph.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(paragraph);
}

/// 模拟打字的停顿：0.5 秒起，每百字加一秒，封顶 `ceiling`。
fn bubble_pause(chars: usize, ceiling: Duration) -> Duration {
    Duration::from_secs_f64(0.5 + chars as f64 / 100.0).min(ceiling)
}

/// 按空行拆成段落；段数多于 `max` 时均衡合并成 `max` 个气泡。
pub(crate) fn split_bubbles(text: &str, max: usize) -> Vec<String> {
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect();
    if paragraphs.len() <= max.max(1) {
        return paragraphs.into_iter().map(str::to_string).collect();
    }
    balanced_groups(&paragraphs, max.max(1))
        .into_iter()
        .map(|group| group.join("\n\n"))
        .collect()
}

/// 线性划分：把段落按顺序分成 `groups` 组，最小化最长一组的长度（连接用的
/// 空行按 2 个字符算）。段落数不多，O(n²·k) 足够。
fn balanced_groups<'a>(paragraphs: &[&'a str], groups: usize) -> Vec<Vec<&'a str>> {
    let n = paragraphs.len();
    let lengths: Vec<usize> = paragraphs.iter().map(|p| p.chars().count()).collect();
    let span = |from: usize, to: usize| -> usize {
        lengths[from..to].iter().sum::<usize>() + 2 * (to - from).saturating_sub(1)
    };
    // best[k][i]：前 i 段分成 k 组时最长一组的最小值；cut 记最后一组的起点。
    let mut best = vec![vec![usize::MAX; n + 1]; groups + 1];
    let mut cut = vec![vec![0usize; n + 1]; groups + 1];
    best[0][0] = 0;
    for k in 1..=groups {
        for i in 1..=n {
            for j in (k - 1)..i {
                if best[k - 1][j] == usize::MAX {
                    continue;
                }
                let cost = best[k - 1][j].max(span(j, i));
                if cost < best[k][i] {
                    best[k][i] = cost;
                    cut[k][i] = j;
                }
            }
        }
    }
    let mut result = Vec::with_capacity(groups);
    let mut end = n;
    for k in (1..=groups).rev() {
        let start = cut[k][end];
        result.push(paragraphs[start..end].to_vec());
        end = start;
    }
    result.reverse();
    result
}

async fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("reading attachment metadata: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("attachment is not a regular file: {}", path.display());
    }
    if metadata.len() > MAX_ATTACHMENT_BYTES as u64 {
        bail!(
            "attachment exceeds the {} MiB connector limit",
            MAX_ATTACHMENT_BYTES / 1024 / 1024
        );
    }
    tokio::fs::read(path)
        .await
        .with_context(|| format!("reading attachment: {}", path.display()))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment".into())
}

fn image_name(mime: &str) -> &'static str {
    match mime {
        "image/png" => "image.png",
        "image/gif" => "image.gif",
        "image/webp" => "image.webp",
        _ => "image.jpg",
    }
}

fn audio_mime(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("ogg") | Some("opus") => "audio/ogg",
        Some("caf") => "audio/x-caf",
        _ => "audio/wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn few_paragraphs_stay_separate() {
        assert_eq!(split_bubbles("a\n\nb\n\n\n\nc", 6), vec!["a", "b", "c"]);
    }

    #[test]
    fn many_paragraphs_merge_evenly() {
        let text = [
            "一二三四五六七八九十",
            "一二",
            "一二",
            "一二三四五六七八九十",
        ]
        .join("\n\n");
        let bubbles = split_bubbles(&text, 2);
        assert_eq!(bubbles.len(), 2);
        assert_eq!(bubbles[0], "一二三四五六七八九十\n\n一二");
        assert_eq!(bubbles[1], "一二\n\n一二三四五六七八九十");
    }

    #[test]
    fn pause_grows_with_length_and_caps() {
        let ceiling = Duration::from_secs(2);
        assert_eq!(bubble_pause(0, ceiling), Duration::from_millis(500));
        assert_eq!(bubble_pause(50, ceiling), Duration::from_secs(1));
        assert_eq!(bubble_pause(1_000, ceiling), ceiling);
    }

    #[test]
    fn audio_mime_follows_extension() {
        assert_eq!(audio_mime("voice.mp3"), "audio/mpeg");
        assert_eq!(audio_mime("voice.WAV"), "audio/wav");
    }
}
