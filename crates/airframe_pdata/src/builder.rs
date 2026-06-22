use crate::bytes::PStoreBytes;
use crate::context::{KeyResolver, PContext};
use crate::typed::PStore;
use airframe_data::cache::ByteCache;
use airframe_data::codec::Codec;

#[derive(Clone)]
pub struct PDataBuilder<BC: ByteCache, R: KeyResolver> {
    bytes: Option<BC>,
    ctx: Option<PContext<R>>,
}

impl<BC: ByteCache, R: KeyResolver> Default for PDataBuilder<BC, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<BC: ByteCache, R: KeyResolver> PDataBuilder<BC, R> {
    pub fn new() -> Self {
        Self {
            bytes: None,
            ctx: None,
        }
    }
    pub fn bytes(mut self, bc: BC) -> Self {
        self.bytes = Some(bc);
        self
    }
    pub fn context(mut self, ctx: PContext<R>) -> Self {
        self.ctx = Some(ctx);
        self
    }

    pub fn build_bytes(self) -> Result<PStoreBytes<BC, R>, crate::error::AirframePdataError> {
        let bc = self
            .bytes
            .ok_or(crate::error::AirframePdataError::InvalidState)?;
        let ctx = self
            .ctx
            .ok_or(crate::error::AirframePdataError::InvalidState)?;
        Ok(PStoreBytes::new(bc, ctx))
    }

    pub fn build_typed<C: Codec>(
        self,
        codec: C,
    ) -> Result<PStore<C, BC, R>, crate::error::AirframePdataError> {
        let bytes = self.build_bytes()?;
        Ok(PStore::new(codec, bytes))
    }
}
