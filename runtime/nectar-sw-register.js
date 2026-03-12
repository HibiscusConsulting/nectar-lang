/**
 * Nectar Service Worker Registration — client-side script for Nectar apps.
 *
 * Provides:
 * - Automatic service worker registration
 * - Update detection with "New version available" signal
 * - NectarSW.update() to force-update the service worker
 * - NectarSW.isOffline reactive signal for offline detection
 *
 * Small enough to inline in HTML.
 */

const NectarSW = (() => {
  let _registration = null;
  let _updateAvailable = false;
  let _isOffline = typeof navigator !== "undefined" ? !navigator.onLine : false;
  const _listeners = { update: [], offline: [] };

  function _notify(type) {
    _listeners[type].forEach((fn) => fn());
  }

  // Offline detection
  if (typeof window !== "undefined") {
    window.addEventListener("online", () => {
      _isOffline = false;
      _notify("offline");
    });
    window.addEventListener("offline", () => {
      _isOffline = true;
      _notify("offline");
    });
  }

  return {
    /** Whether a new service worker version is waiting to activate. */
    get updateAvailable() {
      return _updateAvailable;
    },

    /** Whether the browser is currently offline. */
    get isOffline() {
      return _isOffline;
    },

    /**
     * Register a listener for update or offline state changes.
     * @param {"update"|"offline"} event
     * @param {Function} callback
     */
    on(event, callback) {
      if (_listeners[event]) _listeners[event].push(callback);
    },

    /**
     * Register the Nectar service worker.
     * @param {string} [swUrl="/nectar-sw.js"] - Path to the service worker file.
     * @returns {Promise<ServiceWorkerRegistration|null>}
     */
    async register(swUrl) {
      if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) {
        return null;
      }
      try {
        _registration = await navigator.serviceWorker.register(swUrl || "/nectar-sw.js");

        // Detect waiting worker (update already downloaded)
        if (_registration.waiting) {
          _updateAvailable = true;
          _notify("update");
        }

        // Detect new update becoming available
        _registration.addEventListener("updatefound", () => {
          const newWorker = _registration.installing;
          if (!newWorker) return;
          newWorker.addEventListener("statechange", () => {
            if (newWorker.state === "installed" && navigator.serviceWorker.controller) {
              _updateAvailable = true;
              _notify("update");
            }
          });
        });

        // Listen for controller change (new SW activated) and reload
        navigator.serviceWorker.addEventListener("controllerchange", () => {
          if (_updateAvailable) {
            window.location.reload();
          }
        });

        return _registration;
      } catch (err) {
        console.error("[Nectar SW] Registration failed:", err);
        return null;
      }
    },

    /**
     * Force-activate a waiting service worker update.
     * The page will reload once the new worker takes control.
     */
    update() {
      if (_registration && _registration.waiting) {
        _registration.waiting.postMessage("nectar:skipWaiting");
      } else if (_registration) {
        _registration.update();
      }
    },
  };
})();

// Export for module and window contexts
if (typeof module !== "undefined") {
  module.exports = { NectarSW };
}
if (typeof window !== "undefined") {
  window.NectarSW = NectarSW;
}
