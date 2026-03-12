// runtime/modules/pwa.js — PWA + Gesture + Hardware runtime

const PwaRuntime = {
  registerManifest(memory, jsonPtr, jsonLen) {
    const json = new TextDecoder().decode(new Uint8Array(memory.buffer, jsonPtr, jsonLen));
    const blob = new Blob([json], { type: "application/manifest+json" });
    const url = URL.createObjectURL(blob);
    let link = document.querySelector('link[rel="manifest"]');
    if (!link) {
      link = document.createElement("link");
      link.rel = "manifest";
      document.head.appendChild(link);
    }
    link.href = url;
  },

  cachePrecache(memory, urlsPtr, urlsLen) {
    const urlsJson = new TextDecoder().decode(new Uint8Array(memory.buffer, urlsPtr, urlsLen));
    try {
      const urls = JSON.parse(urlsJson);
      if ("caches" in window) {
        caches.open("nectar-precache-v1").then((cache) => cache.addAll(urls));
      }
    } catch (e) {
      console.warn("[nectar] Failed to parse precache URLs:", e);
    }
  },

  registerServiceWorker(swPath) {
    if ("serviceWorker" in navigator) {
      navigator.serviceWorker.register(swPath || "/nectar-service-worker.js");
    }
  },
};

const GestureRuntime = {
  _handlers: new Map(),
  _nextId: 1,
  _instance: null,

  registerSwipe(elementHandle, direction, callbackIdx) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    let startX = 0, startY = 0;
    const threshold = 50;

    element.addEventListener("touchstart", (e) => {
      startX = e.touches[0].clientX;
      startY = e.touches[0].clientY;
    }, { passive: true });

    element.addEventListener("touchend", (e) => {
      const dx = e.changedTouches[0].clientX - startX;
      const dy = e.changedTouches[0].clientY - startY;
      const absDx = Math.abs(dx);
      const absDy = Math.abs(dy);

      if (absDx > threshold && absDx > absDy) {
        if ((direction === 0 && dx < 0) || (direction === 1 && dx > 0)) {
          if (typeof callbackIdx === "function") callbackIdx();
          else if (GestureRuntime._instance) GestureRuntime._instance.exports.__gesture_callback(callbackIdx);
        }
      } else if (absDy > threshold && absDy > absDx) {
        if ((direction === 2 && dy < 0) || (direction === 3 && dy > 0)) {
          if (typeof callbackIdx === "function") callbackIdx();
          else if (GestureRuntime._instance) GestureRuntime._instance.exports.__gesture_callback(callbackIdx);
        }
      }
    }, { passive: true });
  },

  registerLongPress(elementHandle, callbackIdx, durationMs) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    const duration = durationMs || 500;
    let timer = null;

    element.addEventListener("touchstart", () => {
      timer = setTimeout(() => {
        if (typeof callbackIdx === "function") callbackIdx();
        else if (GestureRuntime._instance) GestureRuntime._instance.exports.__gesture_callback(callbackIdx);
      }, duration);
    }, { passive: true });

    element.addEventListener("touchend", () => {
      if (timer) { clearTimeout(timer); timer = null; }
    }, { passive: true });

    element.addEventListener("touchmove", () => {
      if (timer) { clearTimeout(timer); timer = null; }
    }, { passive: true });
  },

  registerPinch(elementHandle, callbackIdx) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    let initialDistance = null;

    element.addEventListener("touchstart", (e) => {
      if (e.touches.length === 2) {
        const dx = e.touches[0].clientX - e.touches[1].clientX;
        const dy = e.touches[0].clientY - e.touches[1].clientY;
        initialDistance = Math.sqrt(dx * dx + dy * dy);
      }
    }, { passive: true });

    element.addEventListener("touchmove", (e) => {
      if (e.touches.length === 2 && initialDistance !== null) {
        const dx = e.touches[0].clientX - e.touches[1].clientX;
        const dy = e.touches[0].clientY - e.touches[1].clientY;
        const currentDistance = Math.sqrt(dx * dx + dy * dy);
        const scale = currentDistance / initialDistance;
        if (typeof callbackIdx === "function") callbackIdx(scale);
        else if (GestureRuntime._instance) GestureRuntime._instance.exports.__gesture_callback(callbackIdx);
      }
    }, { passive: true });

    element.addEventListener("touchend", () => { initialDistance = null; }, { passive: true });
  },
};

const HardwareRuntime = {
  _instance: null,
  _memory: null,

  haptic(pattern) {
    if (typeof navigator !== "undefined" && navigator.vibrate) {
      navigator.vibrate(pattern);
    }
  },

  biometricAuth(challengePtr, challengeLen, rpPtr, rpLen) {
    if (typeof window === "undefined" || !window.PublicKeyCredential) return -1;
    const challenge = new Uint8Array(HardwareRuntime._memory.buffer, challengePtr, challengeLen);
    const rpId = new TextDecoder().decode(new Uint8Array(HardwareRuntime._memory.buffer, rpPtr, rpLen));
    navigator.credentials.get({
      publicKey: { challenge, rpId, userVerification: "required" },
    }).then(() => {
      if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__biometric_callback(1);
    }).catch(() => {
      if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__biometric_callback(0);
    });
    return 0;
  },

  cameraCapture(facingPtr, facingLen, callbackIdx) {
    const facing = new TextDecoder().decode(new Uint8Array(HardwareRuntime._memory.buffer, facingPtr, facingLen));
    const constraints = { video: { facingMode: facing || "environment" } };
    navigator.mediaDevices.getUserMedia(constraints).then(() => {
      if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__camera_callback(callbackIdx, 1);
    }).catch(() => {
      if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__camera_callback(callbackIdx, 0);
    });
  },

  geolocationCurrent(callbackIdx) {
    if (!navigator.geolocation) {
      if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__geolocation_callback(callbackIdx, 0, 0, 0);
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        if (HardwareRuntime._instance) {
          HardwareRuntime._instance.exports.__geolocation_callback(
            callbackIdx, 1, pos.coords.latitude, pos.coords.longitude
          );
        }
      },
      () => {
        if (HardwareRuntime._instance) HardwareRuntime._instance.exports.__geolocation_callback(callbackIdx, 0, 0, 0);
      }
    );
  },
};

const pwaModule = {
  name: 'pwa',
  runtime: { PwaRuntime, GestureRuntime, HardwareRuntime },
  wasmImports: {
    pwa: {
      registerManifest: PwaRuntime.registerManifest,
      cachePrecache: PwaRuntime.cachePrecache,
      registerServiceWorker: PwaRuntime.registerServiceWorker,
    },
    gesture: {
      registerSwipe: GestureRuntime.registerSwipe,
      registerLongPress: GestureRuntime.registerLongPress,
      registerPinch: GestureRuntime.registerPinch,
    },
    hardware: {
      haptic: HardwareRuntime.haptic,
      biometricAuth: HardwareRuntime.biometricAuth,
      cameraCapture: HardwareRuntime.cameraCapture,
      geolocationCurrent: HardwareRuntime.geolocationCurrent,
    }
  }
};

if (typeof module !== "undefined") module.exports = pwaModule;
