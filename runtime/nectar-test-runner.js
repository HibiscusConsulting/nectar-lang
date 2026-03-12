/**
 * Nectar Test Runner — executes compiled test WASM modules and reports results.
 *
 * Usage:
 *   import { TestRunner } from './nectar-test-runner.js';
 *   const runner = new TestRunner();
 *   const results = await runner.run('./my-tests.wasm');
 *   console.log(results);
 */

import { readFile } from 'node:fs/promises';
import { watch, statSync } from 'node:fs';
import { dirname } from 'node:path';

/**
 * @typedef {Object} TestResult
 * @property {string} name - Test name
 * @property {'pass'|'fail'} status - Test outcome
 * @property {string} [message] - Failure message (only for failed tests)
 */

/**
 * @typedef {Object} TestResults
 * @property {number} passed - Number of passing tests
 * @property {number} failed - Number of failing tests
 * @property {TestResult[]} tests - Individual test results
 */

export class TestRunner {
  constructor(options = {}) {
    /** @type {boolean} */
    this.verbose = options.verbose ?? false;
    /** @type {string|null} */
    this.filterPattern = options.filter ?? null;
    /** @type {TestResult[]} */
    this._results = [];
    /** @type {number} */
    this._passed = 0;
    /** @type {number} */
    this._failed = 0;
    /** @type {WebAssembly.Memory|null} */
    this._memory = null;
  }

  /**
   * Load and execute a compiled Nectar test WASM module.
   * @param {string} wasmPath - Path to the .wasm file
   * @returns {Promise<TestResults>}
   */
  async run(wasmPath) {
    this._results = [];
    this._passed = 0;
    this._failed = 0;

    const wasmBytes = await readFile(wasmPath);
    this._memory = new WebAssembly.Memory({ initial: 1 });

    const importObject = {
      env: {
        memory: this._memory,
      },
      dom: {
        createElement: () => 0,
        setText: () => {},
        appendChild: () => {},
        addEventListener: () => {},
        setAttribute: () => {},
      },
      test: {
        pass: (namePtr, nameLen) => {
          const name = this._readString(namePtr, nameLen);
          this._results.push({ name, status: 'pass' });
          this._passed++;
          if (this.verbose) {
            console.log(`  \x1b[32m✓\x1b[0m ${name}`);
          }
        },
        fail: (namePtr, nameLen, msgPtr, msgLen) => {
          const name = this._readString(namePtr, nameLen);
          const message = this._readString(msgPtr, msgLen);
          this._results.push({ name, status: 'fail', message });
          this._failed++;
          if (this.verbose) {
            console.log(`  \x1b[31m✗\x1b[0m ${name}: ${message}`);
          }
        },
        summary: (passed, failed) => {
          // Summary is called at the end by __run_tests
          if (this.verbose) {
            console.log('');
            if (failed > 0) {
              console.log(`\x1b[31m${failed} failed\x1b[0m, ${passed} passed`);
            } else {
              console.log(`\x1b[32m${passed} passed\x1b[0m`);
            }
          }
        },
      },
      signal: {
        create: () => 0,
        get: () => 0,
        set: () => {},
        subscribe: () => {},
        createEffect: () => {},
        createMemo: () => 0,
        batch: () => {},
      },
      http: {
        fetch: () => 0,
        fetchGetBody: () => 0,
        fetchGetStatus: () => 0,
      },
      worker: {
        spawn: () => 0,
        channelCreate: () => 0,
        channelSend: () => {},
        channelRecv: () => {},
        parallel: () => {},
      },
      ai: {
        chatStream: () => {},
        chatComplete: () => {},
        registerTool: () => {},
        embed: () => {},
        parseStructured: () => 0,
      },
      streaming: {
        streamFetch: () => {},
        sseConnect: () => {},
        wsConnect: () => {},
        wsSend: () => {},
        wsClose: () => {},
        yield: () => {},
      },
      media: {
        lazyImage: () => {},
        decodeImage: () => {},
        preload: () => {},
        progressiveImage: () => {},
      },
      router: {
        init: () => {},
        navigate: () => {},
        currentPath: () => 0,
        getParam: () => 0,
        registerRoute: () => {},
      },
      style: {
        injectStyles: () => 0,
        applyScope: () => {},
      },
      a11y: {
        setAriaAttribute: () => {},
        setRole: () => {},
        manageFocus: () => {},
        announceToScreenReader: () => {},
        trapFocus: () => {},
        releaseFocusTrap: () => {},
      },
    };

    const { instance } = await WebAssembly.instantiate(wasmBytes, importObject);

    // Call __run_tests if it exists
    if (instance.exports.__run_tests) {
      try {
        instance.exports.__run_tests();
      } catch (e) {
        console.error(`\x1b[31mTest execution error:\x1b[0m ${e.message}`);
      }
    } else {
      // Run individual test exports
      for (const [name, fn] of Object.entries(instance.exports)) {
        if (name.startsWith('__test_') && typeof fn === 'function') {
          if (this.filterPattern && !name.includes(this.filterPattern)) {
            continue;
          }
          try {
            fn();
          } catch (e) {
            const testName = name.replace('__test_', '').replace(/_/g, ' ');
            this._results.push({ name: testName, status: 'fail', message: e.message });
            this._failed++;
          }
        }
      }
    }

    return {
      passed: this._passed,
      failed: this._failed,
      tests: this._results,
    };
  }

  /**
   * Read a UTF-8 string from WASM linear memory.
   * @param {number} ptr - Memory offset
   * @param {number} len - Byte length
   * @returns {string}
   */
  _readString(ptr, len) {
    if (len === 0 || !this._memory) return '';
    const bytes = new Uint8Array(this._memory.buffer, ptr, len);
    return new TextDecoder().decode(bytes);
  }

  /**
   * Watch a .wasm file (or source directory) and re-run tests on every change.
   *
   * @param {string} wasmPath - Path to the .wasm file to test
   * @param {Object} [options] - Runner options (verbose, filter)
   * @param {string} [options.sourceDir] - Directory to watch instead of the .wasm file itself
   * @returns {{ stop: () => void }} Handle with a `stop()` method to tear down the watcher
   */
  static watch(wasmPath, options = {}) {
    const DEBOUNCE_MS = 200;
    const watchTarget = options.sourceDir ?? wasmPath;
    const recursive = (() => {
      try { return statSync(watchTarget).isDirectory(); } catch { return false; }
    })();

    let debounceTimer = null;
    let running = false;

    const executeRun = async () => {
      if (running) return;
      running = true;
      try {
        // Clear console and print run header with timestamp
        process.stdout.write('\x1Bc');
        const ts = new Date().toLocaleTimeString();
        console.log(`\x1b[90m[${ts}]\x1b[0m Running tests...\n`);

        const runner = new TestRunner({ verbose: true, filter: options.filter ?? null });
        const results = await runner.run(wasmPath);
        TestRunner.report(results);
      } catch (err) {
        console.error(`\x1b[31mError:\x1b[0m ${err.message}`);
      } finally {
        running = false;
      }
    };

    const scheduleRun = () => {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(executeRun, DEBOUNCE_MS);
    };

    // Initial run
    executeRun();

    const watcher = watch(watchTarget, { recursive }, (_eventType, _filename) => {
      scheduleRun();
    });

    console.log(`\x1b[90mWatching ${watchTarget} for changes. Press Ctrl+C to stop.\x1b[0m\n`);

    const stop = () => {
      if (debounceTimer) clearTimeout(debounceTimer);
      watcher.close();
    };

    // Stop gracefully on SIGINT
    const onSigint = () => {
      console.log('\nStopping watch mode.');
      stop();
      process.exit(0);
    };
    process.on('SIGINT', onSigint);

    return { stop };
  }

  /**
   * Print a formatted test report to the console.
   * @param {TestResults} results
   */
  static report(results) {
    console.log('');
    console.log(`test result: ${results.failed > 0 ? '\x1b[31mFAILED\x1b[0m' : '\x1b[32mok\x1b[0m'}. ` +
      `${results.passed} passed; ${results.failed} failed`);
    console.log('');

    if (results.failed > 0) {
      console.log('failures:');
      for (const test of results.tests) {
        if (test.status === 'fail') {
          console.log(`  \x1b[31m✗\x1b[0m ${test.name}: ${test.message || '(no message)'}`);
        }
      }
      console.log('');
    }
  }
}

// CLI entry point: run as `node nectar-test-runner.js <path.wasm>`
if (process.argv[1]?.endsWith('nectar-test-runner.js')) {
  const wasmPath = process.argv[2];
  if (!wasmPath) {
    console.error('Usage: node nectar-test-runner.js <path.wasm> [--filter <pattern>] [--watch] [--source-dir <dir>]');
    process.exit(1);
  }

  const filterIdx = process.argv.indexOf('--filter');
  const filter = filterIdx !== -1 ? process.argv[filterIdx + 1] : null;
  const watchMode = process.argv.includes('--watch');
  const srcDirIdx = process.argv.indexOf('--source-dir');
  const sourceDir = srcDirIdx !== -1 ? process.argv[srcDirIdx + 1] : null;

  if (watchMode) {
    TestRunner.watch(wasmPath, { filter, sourceDir });
  } else {
    const runner = new TestRunner({ verbose: true, filter });
    runner.run(wasmPath).then((results) => {
      TestRunner.report(results);
      process.exit(results.failed > 0 ? 1 : 0);
    }).catch((err) => {
      console.error(`Error: ${err.message}`);
      process.exit(1);
    });
  }
}
