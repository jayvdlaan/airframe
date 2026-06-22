//! Composite sinks tracing layer that multiplexes to multiple inner fmt layers.

use crate::filters::per_sink::PerSinkFilter;

// Parent subscriber alias lives in a small helper module to avoid circular refs.

pub(crate) struct SinkEntry {
    pub(crate) layer:
        Box<dyn tracing_subscriber::Layer<crate::layer_parent::ParentSubscriber> + Send + Sync>,
    pub(crate) filter: Option<PerSinkFilter>,
}

pub struct SinksLayer {
    pub(crate) inner: Vec<SinkEntry>,
}

impl SinksLayer {
    pub(crate) fn new(inner: Vec<SinkEntry>) -> Self {
        Self { inner }
    }
}

impl tracing_subscriber::Layer<crate::layer_parent::ParentSubscriber> for SinksLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let meta = event.metadata();
        for e in &self.inner {
            if e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true) {
                e.layer.on_event(event, ctx.clone());
            }
        }
    }
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let meta = attrs.metadata();
        for e in &self.inner {
            if e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true) {
                e.layer.on_new_span(attrs, id, ctx.clone());
            }
        }
    }
    fn on_record(
        &self,
        id: &tracing::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let allowed = |e: &SinkEntry| {
            if let Some(span) = ctx.span(id) {
                let meta = span.metadata();
                e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true)
            } else {
                true
            }
        };
        for e in &self.inner {
            if allowed(e) {
                e.layer.on_record(id, values, ctx.clone());
            }
        }
    }
    fn on_enter(
        &self,
        id: &tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let allowed = |e: &SinkEntry| {
            if let Some(span) = ctx.span(id) {
                let meta = span.metadata();
                e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true)
            } else {
                true
            }
        };
        for e in &self.inner {
            if allowed(e) {
                e.layer.on_enter(id, ctx.clone());
            }
        }
    }
    fn on_exit(
        &self,
        id: &tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let allowed = |e: &SinkEntry| {
            if let Some(span) = ctx.span(id) {
                let meta = span.metadata();
                e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true)
            } else {
                true
            }
        };
        for e in &self.inner {
            if allowed(e) {
                e.layer.on_exit(id, ctx.clone());
            }
        }
    }
    fn on_close(
        &self,
        id: tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        let allowed = |e: &SinkEntry| {
            if let Some(span) = ctx.span(&id) {
                let meta = span.metadata();
                e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true)
            } else {
                true
            }
        };
        for e in &self.inner {
            if allowed(e) {
                e.layer.on_close(id.clone(), ctx.clone());
            }
        }
    }
    fn on_id_change(
        &self,
        old: &tracing::Id,
        new: &tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) {
        for e in &self.inner {
            e.layer.on_id_change(old, new, ctx.clone());
        }
    }
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        ctx: tracing_subscriber::layer::Context<'_, crate::layer_parent::ParentSubscriber>,
    ) -> bool {
        // Rely on the upstream EnvFilter to decide if the callsite is enabled.
        // Here we only check per-sink filters; if any sink would accept this metadata, return true.
        let _ = ctx; // not used here; inner layers are expected to respect global filters.
        self.inner
            .iter()
            .any(|e| e.filter.as_ref().map(|f| f.allows(meta)).unwrap_or(true))
    }
    fn register_callsite(
        &self,
        meta: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        let _ = meta;
        // Always register interest; upstream EnvFilter and our `enabled` will decide per-event.
        tracing::subscriber::Interest::always()
    }
    fn max_level_hint(&self) -> Option<tracing::metadata::LevelFilter> {
        // Allow all levels; upstream EnvFilter will actually filter events.
        Some(tracing::metadata::LevelFilter::TRACE)
    }
    fn on_register_dispatch(&self, dispatch: &tracing::Dispatch) {
        for e in &self.inner {
            e.layer.on_register_dispatch(dispatch);
        }
    }
    fn on_layer(&mut self, subscriber: &mut crate::layer_parent::ParentSubscriber) {
        for e in &mut self.inner {
            e.layer.on_layer(subscriber);
        }
    }
}
