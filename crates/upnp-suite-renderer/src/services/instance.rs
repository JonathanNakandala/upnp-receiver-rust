//! A generic, indexed registry of live service instances (`AVTransportID`,
//! `RcsID`, ...), keyed by a small `Copy` id type.
//!
//! AVTransport and RenderingControl each get their own independently
//! numbered registry — per ConnectionManager:3 Annex C.5.6, `AVTransportID`
//! and `RcsID` are independent namespaces, not required to share a value —
//! so this type is generic over the id as well as the stored value, letting
//! a future `ConnectionManager` connection table reuse it too instead of a
//! bespoke map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

/// A UPnP service instance identifier (`AVTransportID`, `RcsID`, ...).
/// Instance 0 always exists for the lifetime of the renderer — the fallback
/// every control point uses when `PrepareForConnection` isn't implemented
/// (or not invoked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(u32);

impl InstanceId {
    pub const DEFAULT: InstanceId = InstanceId(0);

    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for InstanceId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A lookup against an [`InstanceRegistry`] for an id nothing was ever
/// created (or was already removed) under — maps to UPnP error 718,
/// `Invalid InstanceID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid instance id")]
pub struct UnknownInstance;

/// Indexed, concurrently-readable store of live service instances, keyed by
/// `Id`. A registry is created with one pre-populated "default" instance
/// (matching this renderer's current single-instance behavior — control
/// points that never call `PrepareForConnection` always address instance
/// `0`) that can never be removed.
pub struct InstanceRegistry<Id, T: ?Sized> {
    // std (sync) RwLock, not tokio's: the critical section is a plain
    // HashMap read/insert/remove with no `.await` inside it, so a blocking
    // lock held for nanoseconds is strictly cheaper than an async one here
    // (no waker/task-queue machinery) and can't deadlock across `.await`
    // points because it's never held across one. Reads (every action
    // handler, every GetPositionInfo poll) never block each other.
    instances: RwLock<HashMap<Id, Arc<T>>>,
    default_id: Id,
    next_id: AtomicU32,
}

impl<Id, T> InstanceRegistry<Id, T>
where
    Id: Copy + Eq + std::hash::Hash + From<u32>,
    T: ?Sized,
{
    /// Creates a registry with `default_id` pre-populated from `default`.
    /// `default_id` can never be removed via [`Self::remove`].
    pub fn new(default_id: Id, default: Arc<T>) -> Self {
        let mut instances = HashMap::new();
        instances.insert(default_id, default);
        Self {
            instances: RwLock::new(instances),
            default_id,
            next_id: AtomicU32::new(1),
        }
    }

    /// O(1) lookup + cheap `Arc` clone (refcount bump, no state copy) under
    /// a read lock — many concurrent readers never block each other.
    pub fn get(&self, id: Id) -> Result<Arc<T>, UnknownInstance> {
        self.instances
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(UnknownInstance)
    }

    pub fn contains(&self, id: Id) -> bool {
        self.instances.read().unwrap().contains_key(&id)
    }

    /// All currently live instance ids — for `GetCurrentConnectionIDs`-style
    /// reporting once something (a future `PrepareForConnection`) can
    /// actually create more than the default instance.
    pub fn ids(&self) -> Vec<Id> {
        self.instances.read().unwrap().keys().copied().collect()
    }

    /// Allocates the next free id, inserts `instance`, returns its id.
    /// Unused until `PrepareForConnection` exists — this is the seam it
    /// will call into.
    pub fn create(&self, instance: Arc<T>) -> Id {
        let id = Id::from(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.instances.write().unwrap().insert(id, instance);
        id
    }

    /// Removes a non-default instance. The default id can never be removed
    /// — enforced here, not by callers, since it's a service-wide invariant.
    pub fn remove(&self, id: Id) -> Result<(), UnknownInstance> {
        if id == self.default_id {
            return Err(UnknownInstance);
        }
        self.instances
            .write()
            .unwrap()
            .remove(&id)
            .map(|_| ())
            .ok_or(UnknownInstance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_instance_exists_by_construction() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(42u32));
        assert_eq!(*registry.get(InstanceId::DEFAULT).unwrap(), 42);
    }

    #[test]
    fn unknown_id_is_rejected() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(42u32));
        assert_eq!(registry.get(InstanceId::new(1)), Err(UnknownInstance));
    }

    #[test]
    fn create_allocates_unique_growing_ids() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(0u32));
        let first = registry.create(Arc::new(1u32));
        let second = registry.create(Arc::new(2u32));
        assert_ne!(first, second);
        assert_eq!(*registry.get(first).unwrap(), 1);
        assert_eq!(*registry.get(second).unwrap(), 2);
    }

    #[test]
    fn remove_rejects_the_default_id() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(0u32));
        assert_eq!(registry.remove(InstanceId::DEFAULT), Err(UnknownInstance));
        assert!(registry.contains(InstanceId::DEFAULT));
    }

    #[test]
    fn remove_deletes_a_created_instance() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(0u32));
        let id = registry.create(Arc::new(1u32));
        assert!(registry.remove(id).is_ok());
        assert!(!registry.contains(id));
        // Removing again is now an unknown-instance error, not a panic.
        assert_eq!(registry.remove(id), Err(UnknownInstance));
    }

    #[test]
    fn ids_reports_every_live_instance() {
        let registry = InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(0u32));
        let created = registry.create(Arc::new(1u32));
        let mut ids = registry.ids();
        ids.sort();
        assert_eq!(ids, vec![InstanceId::DEFAULT, created]);
    }

    #[tokio::test]
    async fn concurrent_reads_dont_block_each_other() {
        let registry = Arc::new(InstanceRegistry::new(InstanceId::DEFAULT, Arc::new(7u32)));
        let mut handles = Vec::new();
        for _ in 0..64 {
            let registry = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                *registry.get(InstanceId::DEFAULT).unwrap()
            }));
        }
        for handle in handles {
            assert_eq!(handle.await.unwrap(), 7);
        }
    }
}
