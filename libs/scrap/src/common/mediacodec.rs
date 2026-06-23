use hbb_common::{anyhow::Error, bail, log, ResultType};
use ndk::media::media_codec::{MediaCodec, MediaCodecDirection, MediaFormat};
use std::ops::Deref;
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::ImageFormat;
use crate::{CodecFormat, I420ToABGR, I420ToARGB, ImageRgb, VideoDecodePerf};

/// MediaCodec mime type name
const H264_MIME_TYPE: &str = "video/avc";
const H265_MIME_TYPE: &str = "video/hevc";
const COLOR_FORMAT_YUV420_PLANAR: i32 = 19;
// const VP8_MIME_TYPE: &str = "video/x-vnd.on2.vp8";
// const VP9_MIME_TYPE: &str = "video/x-vnd.on2.vp9";

// TODO MediaCodecEncoder

pub static H264_DECODER_SUPPORT: AtomicBool = AtomicBool::new(false);
pub static H265_DECODER_SUPPORT: AtomicBool = AtomicBool::new(false);

pub struct MediaCodecDecoder {
    decoder: MediaCodec,
    name: String,
}

impl Deref for MediaCodecDecoder {
    type Target = MediaCodec;

    fn deref(&self) -> &Self::Target {
        &self.decoder
    }
}

impl MediaCodecDecoder {
    pub fn new(format: CodecFormat) -> Option<MediaCodecDecoder> {
        match format {
            CodecFormat::H264 => create_media_codec(H264_MIME_TYPE, MediaCodecDirection::Decoder),
            CodecFormat::H265 => create_media_codec(H265_MIME_TYPE, MediaCodecDirection::Decoder),
            _ => {
                log::error!("Unsupported codec format: {:?}", format);
                None
            }
        }
    }

    // Android MediaCodec is currently configured for byte-buffer output, not Surface output.
    // The pipeline is MediaCodec -> YUV/I420 output buffer -> CPU I420ToARGB/ABGR -> RGBA soft render.
    // A Surface/Flutter texture path must keep this byte-buffer path as fallback.
    pub fn decode(
        &mut self,
        data: &[u8],
        rgb: &mut ImageRgb,
        perf: &mut VideoDecodePerf,
    ) -> ResultType<bool> {
        perf.codec_path = "android_mediacodec_byte_buffer";
        perf.render_path = "rgba_soft_render";
        perf.input_bytes = data.len();
        let decode_start = Instant::now();
        let input_start = Instant::now();
        match self.dequeue_input_buffer(Duration::from_millis(10))? {
            Some(mut input_buffer) => {
                let mut buf = input_buffer.buffer_mut();
                if data.len() > buf.len() {
                    log::error!("Failed to decode, the input data size is bigger than input buf");
                    bail!("The input data size is bigger than input buf");
                }
                buf.write_all(&data)?;
                self.queue_input_buffer(input_buffer, 0, data.len(), 0, 0)?;
            }
            None => {
                log::debug!("Failed to dequeue_input_buffer: No available input_buffer");
            }
        };
        perf.mediacodec_input_queue = Some(input_start.elapsed());

        let output_start = Instant::now();
        return match self.dequeue_output_buffer(Duration::from_millis(100))? {
            Some(output_buffer) => {
                perf.mediacodec_output_dequeue = Some(output_start.elapsed());
                let res_format = self.output_format();
                let coded_w = res_format
                    .i32("width")
                    .ok_or(Error::msg("Failed to dequeue_output_buffer, width is None"))?;
                let coded_h = res_format.i32("height").ok_or(Error::msg(
                    "Failed to dequeue_output_buffer, height is None",
                ))?;
                let stride = res_format.i32("stride").unwrap_or(coded_w);
                let slice_height = res_format.i32("slice-height").unwrap_or(coded_h);
                let color_format = res_format.i32("color-format");
                let crop_left = res_format.i32("crop-left").unwrap_or(0);
                let crop_top = res_format.i32("crop-top").unwrap_or(0);
                let crop_right = res_format.i32("crop-right").unwrap_or(coded_w - 1);
                let crop_bottom = res_format.i32("crop-bottom").unwrap_or(coded_h - 1);
                if crop_left < 0
                    || crop_top < 0
                    || crop_right < crop_left
                    || crop_bottom < crop_top
                    || crop_right >= coded_w
                    || crop_bottom >= coded_h
                {
                    bail!(
                        "Invalid MediaCodec crop: left={}, top={}, right={}, bottom={}, coded={}x{}",
                        crop_left,
                        crop_top,
                        crop_right,
                        crop_bottom,
                        coded_w,
                        coded_h
                    );
                }
                let w = (crop_right - crop_left + 1).max(0) as usize;
                let h = (crop_bottom - crop_top + 1).max(0) as usize;
                if w == 0 || h == 0 || stride <= 0 || slice_height <= 0 {
                    bail!(
                        "Invalid MediaCodec output format: width={coded_w}, height={coded_h}, stride={stride}, slice_height={slice_height}, crop=({}, {}, {}, {})",
                        crop_left,
                        crop_top,
                        crop_right,
                        crop_bottom
                    );
                }
                if color_format.is_some() && color_format != Some(COLOR_FORMAT_YUV420_PLANAR) {
                    log::warn!(
                        "diag android mediacodec unsupported byte-buffer color format: decoder={}, color_format={:?}, requested={}, width={}, height={}, stride={}, slice_height={}, fallback=mark_decoder_unsupported",
                        self.name,
                        color_format,
                        COLOR_FORMAT_YUV420_PLANAR,
                        coded_w,
                        coded_h,
                        stride,
                        slice_height
                    );
                    bail!(
                        "Unsupported MediaCodec byte-buffer color format: {:?}",
                        color_format
                    );
                }
                let buf = output_buffer.buffer();
                let bps = 4;
                let dst_align = rgb.align().max(1);
                let dst_stride = if dst_align == 1 {
                    w * bps
                } else {
                    (w * bps + dst_align - 1) & !(dst_align - 1)
                };
                let required_rgba_len = h * dst_stride;
                let old_capacity = rgb.raw.capacity();
                if rgb.raw.len() != required_rgba_len {
                    rgb.raw.resize(required_rgba_len, 0);
                }
                rgb.w = w;
                rgb.h = h;
                perf.media_format_width = Some(coded_w);
                perf.media_format_height = Some(coded_h);
                perf.media_format_stride = Some(stride);
                perf.media_format_slice_height = Some(slice_height);
                perf.media_format_color_format = color_format;
                perf.crop_left = Some(crop_left);
                perf.crop_top = Some(crop_top);
                perf.crop_right = Some(crop_right);
                perf.crop_bottom = Some(crop_bottom);
                perf.width = w;
                perf.height = h;
                perf.output_buffer_bytes = buf.len();
                perf.rgba_bytes = rgb.raw.len();
                perf.rgba_reallocated = old_capacity < required_rgba_len;

                let y_stride = stride as usize;
                let uv_stride = y_stride / 2;
                let y_plane_len = y_stride * slice_height as usize;
                let uv_plane_height = (slice_height as usize + 1) / 2;
                let u_offset = y_plane_len;
                let v_offset = u_offset + uv_stride * uv_plane_height;
                let y_crop_offset = crop_top as usize * y_stride + crop_left as usize;
                let uv_crop_offset = (crop_top as usize / 2) * uv_stride + crop_left as usize / 2;
                let uv_width = (w + 1) / 2;
                let uv_height = (h + 1) / 2;
                let y_end = y_crop_offset + (h.saturating_sub(1)) * y_stride + w;
                let u_end = u_offset
                    + uv_crop_offset
                    + (uv_height.saturating_sub(1)) * uv_stride
                    + uv_width;
                let v_end = v_offset
                    + uv_crop_offset
                    + (uv_height.saturating_sub(1)) * uv_stride
                    + uv_width;
                if uv_stride == 0
                    || y_crop_offset >= u_offset
                    || y_end > buf.len()
                    || u_end > buf.len()
                    || v_end > buf.len()
                {
                    bail!(
                        "Invalid MediaCodec I420 plane offsets: buf_len={}, y_offset={}, y_end={}, u_offset={}, u_end={}, v_offset={}, v_end={}, uv_stride={}, width={}, height={}, stride={}, slice_height={}",
                        buf.len(),
                        y_crop_offset,
                        y_end,
                        u_offset + uv_crop_offset,
                        u_end,
                        v_offset + uv_crop_offset,
                        v_end,
                        uv_stride,
                        w,
                        h,
                        stride,
                        slice_height
                    );
                }
                let y_ptr = unsafe { buf.as_ptr().add(y_crop_offset) };
                let u_ptr = unsafe { buf.as_ptr().add(u_offset + uv_crop_offset) };
                let v_ptr = unsafe { buf.as_ptr().add(v_offset + uv_crop_offset) };
                let convert_start = Instant::now();
                unsafe {
                    match rgb.fmt() {
                        ImageFormat::ARGB => {
                            I420ToARGB(
                                y_ptr,
                                stride,
                                u_ptr,
                                stride / 2,
                                v_ptr,
                                stride / 2,
                                rgb.raw.as_mut_ptr(),
                                dst_stride as _,
                                w as _,
                                h as _,
                            );
                        }
                        ImageFormat::ABGR => {
                            I420ToABGR(
                                y_ptr,
                                stride,
                                u_ptr,
                                stride / 2,
                                v_ptr,
                                stride / 2,
                                rgb.raw.as_mut_ptr(),
                                dst_stride as _,
                                w as _,
                                h as _,
                            );
                        }
                        _ => {
                            bail!("Unsupported image format");
                        }
                    }
                }
                perf.yuv_to_rgba = Some(convert_start.elapsed());
                perf.decoder_total = Some(decode_start.elapsed());
                self.release_output_buffer(output_buffer, false)?;
                Ok(true)
            }
            None => {
                perf.mediacodec_output_dequeue = Some(output_start.elapsed());
                perf.decoder_total = Some(decode_start.elapsed());
                log::debug!("Failed to dequeue_output: No available dequeue_output");
                Ok(false)
            }
        };
    }
}

fn create_media_codec(name: &str, direction: MediaCodecDirection) -> Option<MediaCodecDecoder> {
    let codec = MediaCodec::from_decoder_type(name)?;
    let media_format = MediaFormat::new();
    media_format.set_str("mime", name);
    // Width/height are stream-defined for this legacy byte-buffer decoder path.
    // The actual decoded size is read from output_format() and logged per frame.
    media_format.set_i32("width", 0);
    media_format.set_i32("height", 0);
    media_format.set_i32("color-format", COLOR_FORMAT_YUV420_PLANAR);
    log::info!(
        "diag android mediacodec init: mime={}, requested_color_format={}, output=byte_buffer, render_path=rgba_soft_render, texture_path=unavailable, fallback=rgba_soft_render",
        name,
        COLOR_FORMAT_YUV420_PLANAR
    );
    if let Err(e) = codec.configure(&media_format, None, direction) {
        log::error!("Failed to init decoder: {:?}", e);
        return None;
    };
    log::info!("MediaCodec decoder configure success: {}", name);
    if let Err(e) = codec.start() {
        log::error!("Failed to start decoder: {:?}", e);
        return None;
    };
    log::debug!("Init decoder succeeded!: {:?}", name);
    return Some(MediaCodecDecoder {
        decoder: codec,
        name: name.to_owned(),
    });
}

pub fn check_mediacodec() {
    std::thread::spawn(move || {
        // check decoders
        let h264 = MediaCodecDecoder::new(CodecFormat::H264);
        let h265 = MediaCodecDecoder::new(CodecFormat::H265);
        H264_DECODER_SUPPORT.swap(h264.is_some(), Ordering::SeqCst);
        H265_DECODER_SUPPORT.swap(h265.is_some(), Ordering::SeqCst);
        if let Some(decoder) = h264 {
            decoder.stop().ok();
        }
        if let Some(decoder) = h265 {
            decoder.stop().ok();
        }
        // TODO encoders
    });
}
