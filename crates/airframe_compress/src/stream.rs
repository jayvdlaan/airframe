use crate::compressor::Compressor;
use crate::AirframeCompressError;
use std::io::{Read, Result as IoResult, Write};

// Fallback (no backends enabled): expose API that returns Unsupported errors so the crate compiles with default = []
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
pub struct CompressWriter<W: Write> {
    _phantom: std::marker::PhantomData<W>,
}
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
impl<W: Write> CompressWriter<W> {
    pub fn into_inner(self) -> Result<W, AirframeCompressError> {
        Err(AirframeCompressError::Unsupported(
            "no compression backends enabled".into(),
        ))
    }
}
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
impl<W: Write> Write for CompressWriter<W> {
    fn write(&mut self, _buf: &[u8]) -> IoResult<usize> {
        Err(std::io::Error::other("no compression backends enabled"))
    }
    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
pub fn new_compress_writer<W: Write>(
    _algo: &dyn Compressor,
    _w: W,
) -> Result<CompressWriter<W>, AirframeCompressError> {
    tracing::warn!(target = "airframe_compress", "algorithm not enabled");
    Err(AirframeCompressError::Unsupported(
        "no compression backends enabled".into(),
    ))
}
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
pub struct DecompressReader;
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
impl Read for DecompressReader {
    fn read(&mut self, _buf: &mut [u8]) -> IoResult<usize> {
        Err(std::io::Error::other("no compression backends enabled"))
    }
}
#[cfg(not(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
)))]
pub fn new_decompress_reader<R: Read + 'static>(
    _algo: &dyn Compressor,
    _r: R,
) -> Result<DecompressReader, AirframeCompressError> {
    tracing::warn!(target = "airframe_compress", "algorithm not enabled");
    Err(AirframeCompressError::Unsupported(
        "no compression backends enabled".into(),
    ))
}

// Real implementation when at least one backend is enabled
#[cfg(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(
        feature = "zstd",
        feature = "lz4",
        feature = "gzip",
        feature = "brotli"
    )))
)]
mod with_backends {
    use super::*;
    use tracing::{debug, error, warn};

    #[cfg(feature = "zstd")]
    fn zstd_level(v: Option<i32>) -> i32 {
        v.unwrap_or(0)
    }

    pub enum CompressWriterImpl<W: Write> {
        #[cfg(feature = "zstd")]
        Zstd(zstd::stream::write::Encoder<'static, W>),
        #[cfg(feature = "lz4")]
        Lz4(lz4_flex::frame::FrameEncoder<W>),
        #[cfg(feature = "gzip")]
        Gzip(flate2::write::GzEncoder<W>),
        #[cfg(feature = "brotli")]
        Brotli(brotli::CompressorWriter<W>),
    }

    pub struct CompressWriter<W: Write> {
        pub(super) inner: CompressWriterImpl<W>,
    }

    impl<W: Write> CompressWriter<W> {
        pub fn into_inner(self) -> Result<W, AirframeCompressError> {
            match self.inner {
                #[cfg(feature = "zstd")]
                CompressWriterImpl::Zstd(enc) => enc
                    .finish()
                    .map_err(|e| AirframeCompressError::CompressError(format!("zstd: {}", e))),
                #[cfg(feature = "lz4")]
                CompressWriterImpl::Lz4(enc) => enc
                    .finish()
                    .map_err(|e| AirframeCompressError::CompressError(format!("lz4: {}", e))),
                #[cfg(feature = "gzip")]
                CompressWriterImpl::Gzip(enc) => enc
                    .finish()
                    .map_err(|e| AirframeCompressError::CompressError(format!("gzip: {}", e))),
                #[cfg(feature = "brotli")]
                CompressWriterImpl::Brotli(enc) => Ok(enc.into_inner()),
            }
        }
    }

    impl<W: Write> Write for CompressWriter<W> {
        fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
            match &mut self.inner {
                #[cfg(feature = "zstd")]
                CompressWriterImpl::Zstd(w) => w.write(buf),
                #[cfg(feature = "lz4")]
                CompressWriterImpl::Lz4(w) => w.write(buf),
                #[cfg(feature = "gzip")]
                CompressWriterImpl::Gzip(w) => w.write(buf),
                #[cfg(feature = "brotli")]
                CompressWriterImpl::Brotli(w) => w.write(buf),
            }
        }
        fn flush(&mut self) -> IoResult<()> {
            match &mut self.inner {
                #[cfg(feature = "zstd")]
                CompressWriterImpl::Zstd(w) => w.flush(),
                #[cfg(feature = "lz4")]
                CompressWriterImpl::Lz4(w) => w.flush(),
                #[cfg(feature = "gzip")]
                CompressWriterImpl::Gzip(w) => w.flush(),
                #[cfg(feature = "brotli")]
                CompressWriterImpl::Brotli(w) => w.flush(),
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(algo, w))]
    pub fn new_compress_writer<W: Write>(
        algo: &dyn Compressor,
        w: W,
    ) -> Result<CompressWriter<W>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %algo.name(), "new_compress_writer");
        let inner = match algo.name() {
            #[cfg(feature = "zstd")]
            "zstd" => {
                let enc = zstd::stream::write::Encoder::new(w, zstd_level(algo.level()))
                    .map_err(|e| {
                        error!(target = "airframe_compress", error = ?e, algo = %algo.name(), "new_compress_writer failed");
                        AirframeCompressError::CompressError(format!("zstd: {}", e))
                    })?;
                CompressWriterImpl::Zstd(enc)
            }
            #[cfg(feature = "lz4")]
            "lz4" => CompressWriterImpl::Lz4(lz4_flex::frame::FrameEncoder::new(w)),
            #[cfg(feature = "gzip")]
            "gzip" => {
                let level = algo.level().unwrap_or(6) as u32;
                CompressWriterImpl::Gzip(flate2::write::GzEncoder::new(
                    w,
                    flate2::Compression::new(level),
                ))
            }
            #[cfg(feature = "brotli")]
            "brotli" => {
                let quality = algo.level().unwrap_or(5) as u32;
                let lgwin = 22u32;
                CompressWriterImpl::Brotli(brotli::CompressorWriter::new(w, 4096, quality, lgwin))
            }
            _ => {
                warn!(target = "airframe_compress", algo = %algo.name(), "algorithm not enabled");
                return Err(AirframeCompressError::Unsupported(format!(
                    "streaming not supported for {}",
                    algo.name()
                )));
            }
        };
        Ok(CompressWriter { inner })
    }

    pub struct DecompressReader {
        inner: Box<dyn Read>,
    }

    impl Read for DecompressReader {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
            self.inner.read(buf)
        }
    }

    #[tracing::instrument(level = "trace", skip(algo, r))]
    pub fn new_decompress_reader<R: Read + 'static>(
        algo: &dyn Compressor,
        r: R,
    ) -> Result<DecompressReader, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %algo.name(), "new_decompress_reader");
        let inner: Box<dyn Read> = match algo.name() {
            #[cfg(feature = "zstd")]
            "zstd" => {
                let dec = zstd::stream::read::Decoder::new(r)
                    .map_err(|e| {
                        error!(target = "airframe_compress", error = ?e, algo = %algo.name(), "new_decompress_reader failed");
                        AirframeCompressError::DecompressError(format!("zstd: {}", e))
                    })?;
                Box::new(dec)
            }
            #[cfg(feature = "lz4")]
            "lz4" => Box::new(lz4_flex::frame::FrameDecoder::new(r)),
            #[cfg(feature = "gzip")]
            "gzip" => Box::new(flate2::read::GzDecoder::new(r)),
            #[cfg(feature = "brotli")]
            "brotli" => Box::new(brotli::Decompressor::new(r, 4096)),
            _ => {
                warn!(target = "airframe_compress", algo = %algo.name(), "algorithm not enabled");
                return Err(AirframeCompressError::Unsupported(format!(
                    "streaming not supported for {}",
                    algo.name()
                )));
            }
        };
        Ok(DecompressReader { inner })
    }
}

// Re-export symbols from the backend module when enabled so public API stays the same
#[cfg(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
))]
pub use with_backends::*;

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyAlgo;
    impl Compressor for DummyAlgo {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn default_extension(&self) -> &'static str {
            "dmy"
        }
        fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
        fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
    }

    #[cfg(not(any(
        feature = "zstd",
        feature = "lz4",
        feature = "gzip",
        feature = "brotli"
    )))]
    #[test]
    fn no_backends_returns_unsupported() {
        let algo = DummyAlgo;
        let sink = Vec::new();
        let err = new_compress_writer(&algo, sink).err().unwrap();
        match err {
            AirframeCompressError::Unsupported(_) => {}
            _ => panic!("expected Unsupported"),
        }

        // Reader
        let src: &[u8] = &[];
        let err = new_decompress_reader(&algo, src).err().unwrap();
        match err {
            AirframeCompressError::Unsupported(_) => {}
            _ => panic!("expected Unsupported"),
        }
    }

    // With backend enabled we can at least exercise the Unsupported branch for an unknown algo name
    #[cfg(any(
        feature = "zstd",
        feature = "lz4",
        feature = "gzip",
        feature = "brotli"
    ))]
    #[test]
    fn unknown_algo_is_unsupported_with_backends() {
        let algo = DummyAlgo; // name "dummy" not mapped
        let sink = Vec::new();
        let err = new_compress_writer(&algo, sink).err().unwrap();
        match err {
            AirframeCompressError::Unsupported(msg) => assert!(msg.contains("dummy")),
            _ => panic!("expected Unsupported"),
        }

        let src: &[u8] = &[];
        let err = new_decompress_reader(&algo, src).err().unwrap();
        match err {
            AirframeCompressError::Unsupported(msg) => assert!(msg.contains("dummy")),
            _ => panic!("expected Unsupported"),
        }
    }
}
