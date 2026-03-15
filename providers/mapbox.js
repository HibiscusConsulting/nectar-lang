// providers/mapbox.js — Mapbox map provider for Nectar
// Overrides the generic map.mount/add_marker stubs in core.js with Mapbox GL JS calls.
// Loaded when `provider: "mapbox"` is declared.
//
// Usage: include this file alongside core.js in the build output.
// The build system does this automatically based on `required_providers`.

export function register(wasmImports, R) {
  wasmImports.map.mount = function(elId, keyPtr, keyLen, lat, lng, zoom, cbIdx) {
    const s = document.createElement('script');
    s.src = 'https://api.mapbox.com/mapbox-gl-js/v2.15.0/mapbox-gl.js';
    s.onload = () => {
      mapboxgl.accessToken = R.__getString(keyPtr, keyLen);
      R.__cbData(cbIdx, R.__registerObject(new mapboxgl.Map({
        container: R.__getElement(elId),
        style: 'mapbox://styles/mapbox/streets-v12',
        center: [lng, lat],
        zoom: zoom,
      })));
    };
    s.onerror = () => R.__cbData(cbIdx, 0);
    document.head.appendChild(s);
  };

  wasmImports.map.add_marker = function(mapId, lat, lng) {
    new mapboxgl.Marker().setLngLat([lng, lat]).addTo(R.__getObject(mapId));
  };
}
