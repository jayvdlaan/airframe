use airframe_core::bus::EventBus;
use airframe_event::Tick;
use futures::StreamExt;

#[tokio::test]
async fn single_subscriber_receives_in_order() {
    let bus = airframe_core::bus::inmem::InMemoryEventBus::new();
    let mut sub = bus.subscribe::<Tick>().unwrap();

    // Publish a few ticks
    for i in 0..5u64 {
        bus.publish(Tick(i), None).await.unwrap();
    }

    // Receive in order
    for i in 0..5u64 {
        let evt = sub.next().await.unwrap();
        assert_eq!(evt, Tick(i));
    }
}

#[tokio::test]
async fn multiple_subscribers_receive_same_events() {
    let bus = airframe_core::bus::inmem::InMemoryEventBus::new();
    let mut a = bus.subscribe::<Tick>().unwrap();
    let mut b = bus.subscribe::<Tick>().unwrap();

    bus.publish(Tick(42), None).await.unwrap();

    let ea = a.next().await.unwrap();
    let eb = b.next().await.unwrap();
    assert_eq!(ea, Tick(42));
    assert_eq!(eb, Tick(42));
}

#[tokio::test]
async fn publish_without_subscribers_is_ok() {
    let bus = airframe_core::bus::inmem::InMemoryEventBus::new();
    // No subscribers yet
    bus.publish(Tick(1), None).await.unwrap();
    // Now subscribe; should not receive past events (broadcast semantics)
    let mut sub = bus.subscribe::<Tick>().unwrap();
    // Publish one more and ensure we get it
    bus.publish(Tick(2), None).await.unwrap();
    let evt = sub.next().await.unwrap();
    assert_eq!(evt, Tick(2));
}
