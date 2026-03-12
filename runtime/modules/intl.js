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
    // WASM pre-computes (value, unit) — JS only calls Intl.RelativeTimeFormat.format()
    formatRelativeTime(value, unitPtr, unitLen, localePtr, localeLen) {
      const unit = NectarRuntime.__getString(unitPtr, unitLen);
      const locale = NectarRuntime.__getString(localePtr, localeLen) || 'en';
      const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
      return NectarRuntime.__allocString(rtf.format(value, unit));
    },
  },
};
