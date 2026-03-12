// runtime/modules/time.js — Temporal types runtime (Instant, ZonedDateTime, Duration)

const TimeRuntime = {
  now() {
    return BigInt(Date.now());
  },

  format(readString, instantMs, patternPtr, patternLen) {
    const pattern = readString(patternPtr, patternLen);
    const date = new Date(Number(instantMs));

    let options = {};
    switch (pattern) {
      case 'iso': return date.toISOString();
      case 'date': options = { year: 'numeric', month: '2-digit', day: '2-digit' }; break;
      case 'time': options = { hour: '2-digit', minute: '2-digit', second: '2-digit' }; break;
      case 'datetime':
        options = { year: 'numeric', month: '2-digit', day: '2-digit',
                    hour: '2-digit', minute: '2-digit', second: '2-digit' };
        break;
      default: return date.toLocaleString();
    }
    return new Intl.DateTimeFormat(undefined, options).format(date);
  },

  toZone(readString, instantMs, tzPtr, tzLen) {
    const tz = readString(tzPtr, tzLen);
    try {
      new Intl.DateTimeFormat('en-US', {
        timeZone: tz,
        year: 'numeric', month: 'numeric', day: 'numeric',
        hour: 'numeric', minute: 'numeric', second: 'numeric',
      });
      return instantMs;
    } catch (e) {
      return instantMs;
    }
  },

  addDuration(instantMs, durationMs) {
    return instantMs + durationMs;
  },
};

const timeModule = {
  name: 'time',
  runtime: TimeRuntime,
  wasmImports: {
    time: {
      now: TimeRuntime.now,
      format: TimeRuntime.format,
      toZone: TimeRuntime.toZone,
      addDuration: TimeRuntime.addDuration,
    }
  }
};

if (typeof module !== "undefined") module.exports = timeModule;
