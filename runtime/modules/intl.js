// intl.js — Thin bridge to browser Intl APIs
// This is the ONLY JS bridge for the standard library.
// Everything else runs as pure WASM compiled from Rust.
// These 3 functions exist because WASM cannot access Intl directly.

export const name = 'intl';

export const runtime = ``;

export const wasmImports = {
  intl: {
    formatNumber(value, localePtr, localeLen) {
      const locale = NectarRuntime.__getString(localePtr, localeLen) || 'en-US';
      return NectarRuntime.__allocString(
        new Intl.NumberFormat(locale).format(value)
      );
    },
    formatCurrency(value, currPtr, currLen, localePtr, localeLen) {
      const currency = NectarRuntime.__getString(currPtr, currLen) || 'USD';
      const locale = NectarRuntime.__getString(localePtr, localeLen) || 'en-US';
      return NectarRuntime.__allocString(
        new Intl.NumberFormat(locale, { style: 'currency', currency }).format(value)
      );
    },
    formatRelativeTime(timestampMs) {
      const diff = Date.now() - Number(timestampMs);
      const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
      const seconds = Math.floor(diff / 1000);
      if (Math.abs(seconds) < 60) return NectarRuntime.__allocString(rtf.format(-seconds, 'second'));
      const minutes = Math.floor(seconds / 60);
      if (Math.abs(minutes) < 60) return NectarRuntime.__allocString(rtf.format(-minutes, 'minute'));
      const hours = Math.floor(minutes / 60);
      if (Math.abs(hours) < 24) return NectarRuntime.__allocString(rtf.format(-hours, 'hour'));
      const days = Math.floor(hours / 24);
      return NectarRuntime.__allocString(rtf.format(-days, 'day'));
    },
  },
};
