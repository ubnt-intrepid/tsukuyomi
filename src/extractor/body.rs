//! Extractors for parsing message body.

use std::fmt;
use std::marker::PhantomData;
use std::str;

use bytes::Bytes;
use http::StatusCode;
use mime::Mime;
use serde::de::DeserializeOwned;

use crate::error::HttpError;
use crate::extractor::{Extractor, Preflight};
use crate::input::Input;

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractBodyError {
    #[fail(display = "missing the header field `Content-type`")]
    MissingContentType,

    #[fail(
        display = "the header field `Content-type` is not an expected value (expected: {})",
        expected
    )]
    UnexpectedContentType { expected: &'static str },

    #[fail(display = "the header field `Content-type` is not a valid MIME")]
    InvalidMime,

    #[fail(display = "charset in `Content-type` must be equal to `utf-8`")]
    NotUtf8Charset,

    #[fail(
        display = "the content of message body is invalid: {}",
        cause
    )]
    InvalidContent { cause: failure::Error },
}

impl HttpError for ExtractBodyError {
    fn status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

fn get_mime_opt<'a>(input: &'a mut Input<'_>) -> Result<Option<&'a Mime>, ExtractBodyError> {
    crate::input::header::content_type(input).map_err(|_| ExtractBodyError::InvalidMime)
}

mod decode {
    use super::*;

    pub trait Decoder<T> {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError>;
        fn decode(data: &Bytes) -> Result<T, ExtractBodyError>;
    }

    #[derive(Debug, Default)]
    pub struct PlainTextDecoder(());

    impl<T> Decoder<T> for PlainTextDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            if let Some(mime) = mime {
                if mime.type_() != mime::TEXT || mime.subtype() != mime::PLAIN {
                    return Err(ExtractBodyError::UnexpectedContentType {
                        expected: "text/plain",
                    });
                }
                if let Some(charset) = mime.get_param("charset") {
                    if charset != "utf-8" {
                        return Err(ExtractBodyError::NotUtf8Charset);
                    }
                }
            }
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            let s = str::from_utf8(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })?;
            serde_plain::from_str(s).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    #[derive(Debug, Default)]
    pub struct JsonDecoder(());

    impl<T> Decoder<T> for JsonDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            let mime = mime.ok_or_else(|| ExtractBodyError::MissingContentType)?;
            if *mime != mime::APPLICATION_JSON {
                return Err(ExtractBodyError::UnexpectedContentType {
                    expected: "application/json",
                });
            }
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            serde_json::from_slice(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    #[derive(Debug, Default)]
    pub struct UrlencodedDecoder(());

    impl<T> Decoder<T> for UrlencodedDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            let mime = mime.ok_or_else(|| ExtractBodyError::MissingContentType)?;
            if *mime != mime::APPLICATION_WWW_FORM_URLENCODED {
                return Err(ExtractBodyError::UnexpectedContentType {
                    expected: "application/x-www-form-urlencoded",
                });
            }
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            serde_urlencoded::from_bytes(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }
}

pub type Plain<T> = Body<T, self::decode::PlainTextDecoder>;
pub type Json<T> = Body<T, self::decode::JsonDecoder>;
pub type Urlencoded<T> = Body<T, self::decode::UrlencodedDecoder>;

/// The instance of `FromInput` which deserializes the message body to the specified type.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct Body<T, D: self::decode::Decoder<T>> {
    decoder: D,
    _marker: PhantomData<fn() -> T>,
}

impl<T, D> Default for Body<T, D>
where
    D: self::decode::Decoder<T> + Default,
{
    fn default() -> Self {
        Self {
            decoder: D::default(),
            _marker: PhantomData,
        }
    }
}

impl<T, D> fmt::Debug for Body<T, D>
where
    D: self::decode::Decoder<T> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body")
            .field("decoder", &self.decoder)
            .finish()
    }
}

impl<T, D> Extractor for Body<T, D>
where
    D: self::decode::Decoder<T>,
{
    type Out = T;
    type Error = ExtractBodyError;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let mime_opt = get_mime_opt(input)?;
        self.decoder.validate_mime(mime_opt)?;
        Ok(Preflight::Incomplete(()))
    }

    #[inline]
    fn finalize(_: Self::Ctx, _: &mut Input<'_>, data: &Bytes) -> Result<Self::Out, Self::Error> {
        D::decode(data)
    }
}