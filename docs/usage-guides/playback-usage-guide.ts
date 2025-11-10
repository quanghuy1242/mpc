/**
 * TypeScript Usage Guide for core-playback WASM
 *
 * This guide demonstrates how to use the audio playback and decoding functionality
 * exposed by core-playback WASM module.
 *
 * Features:
 * - Audio format probing (MP3, AAC, FLAC, etc.)
 * - High-performance decoding to PCM
 * - Batch decoding for reduced WASM/JS boundary crossings
 * - Seeking support
 * - Cache configuration
 * - Feature detection
 *
 * Build Configuration:
 * - This bundle includes: MP3, AAC, WAV decoders
 * - FLAC, Vorbis, Opus available as separate lazy-loadable bundles
 */

import init, {
  // Decoder
  JsAudioDecoder,
  JsProbeResult,
  JsAudioFormat,
  JsAudioCodec,
  
  // Cache
  JsCacheConfig,
  JsPlaybackCacheStatus,
  JsOfflineCacheManager,
  JsStorageProviderConfig,
  
  // Streaming
  JsStreamingConfig,
  JsStreamingSession,
  JsRingBuffer,
  JsAudioSource,
  JsStreamingState,
  JsStreamingStats,
  
  // Utilities
  playback_version,
  playback_name,
  supported_formats,
  is_format_supported,
  has_offline_cache,
  has_http_streaming,
  has_decoder,
  
  // Logging (from core-runtime)
  initLogging,
  JsLoggingConfig,
} from '../../core-playback/pkg/core_playback.js';

import { JsHttpClient } from '../../bridge-wasm/pkg/bridge_wasm.js';
import { JsLibrary } from '../../core-library/pkg/core_library.js';
import { JsEventBus } from '../../core-runtime/pkg/core_runtime.js';

// ============================================================================ 
// 1. Initialize WASM
// ============================================================================ 

async function initializeWasm() {
  console.log('=== Initializing core-playback WASM ===\n');
  
  await init();
  
  console.log(`Module: ${playback_name()}`);
  console.log(`Version: ${playback_version()}`);
  console.log('\n✓ WASM initialized\n');
}

// ============================================================================ 
// 2. Feature Detection
// ============================================================================ 

function checkFeatures() {
  console.log('=== Feature Detection ===\n');
  
  console.log(`Decoder available: ${has_decoder()}`);
  console.log(`Offline cache: ${has_offline_cache()}`);
  console.log(`HTTP streaming: ${has_http_streaming()}`);
  
  console.log('\nSupported formats:');
  const formats = supported_formats();
  formats.forEach(fmt => console.log(`  ✓ ${fmt}`));
  
  console.log('\n✓ Feature detection complete\n');
}

// ============================================================================ 
// 3. Audio Decoding - Basic Usage
// ============================================================================ 

async function basicDecoding() {
  console.log('=== Basic Audio Decoding ===\n');
  
  // Simulate loading an audio file
  // In real app, you'd fetch from URL or IndexedDB
  const audioData = await fetch('/path/to/song.mp3')
    .then(res => res.arrayBuffer())
    .then(buf => new Uint8Array(buf));
  
  // Create decoder
  console.log('Creating decoder...');
  const decoder = await JsAudioDecoder.create(audioData, 'song.mp3');
  
  // Get format information
  const probe = decoder.getFormat();
  const format = probe.format();
  const durationMs = probe.durationMs();
  console.log(`Sample rate: ${format.sampleRate()} Hz`);
  console.log(`Channels: ${format.channels()}`);
  console.log(`Duration: ${durationMs !== undefined ? (Number(durationMs) / 1000).toFixed(2) : 'unknown'} seconds`);
  
  // Decode first chunk
  console.log('\nDecoding first chunk (4096 frames)...');
  const samples = await decoder.decodeFrames(4096);
  
  if (samples !== null) {
    console.log(`Decoded ${samples.length} samples (interleaved)`);
    const firstSamples = Array.from(samples.slice(0, 10)) as number[];
    console.log(`First few samples: ${firstSamples.map(s => s.toFixed(4)).join(', ')}`);
  }
  
  console.log('\n✓ Basic decoding complete\n');
}

// ============================================================================ 
// 4. Batch Decoding (High Performance)
// ============================================================================ 

async function batchDecoding() {
  console.log('=== Batch Decoding (Optimized) ===\n');
  
  const audioData = await fetch('/path/to/song.aac')
    .then(res => res.arrayBuffer())
    .then(buf => new Uint8Array(buf));
  
  const decoder = await JsAudioDecoder.create(audioData, 'song.aac');
  const probe = decoder.getFormat();
  const format = probe.format();
  
  console.log('Decoding 1 second of audio in one batch...');
  console.time('batch-decode');
  
  // Decode ~1 second worth of frames in one call
  // This is 40x faster than calling decodeFrames repeatedly!
  const targetFrames = format.sampleRate(); // 1 second
  const maxPackets = 50; // Prevent infinite loops
  
  const samples = await decoder.decodeBatch(targetFrames, maxPackets);
  console.timeEnd('batch-decode');
  
  if (samples !== null) {
    const actualFrames = samples.length / format.channels();
    console.log(`Decoded ${actualFrames} frames (${(actualFrames / format.sampleRate()).toFixed(2)}s)`);
    console.log(`Samples: ${samples.length} (interleaved)`);
  }
  
  console.log('\n✓ Batch decoding complete\n');
}

// ============================================================================ 
// 5. Complete Decode to Web Audio API
// ============================================================================ 

async function decodeAndPlay() {
  console.log('=== Decode and Play via Web Audio API ===\n');
  
  // Fetch audio file
  const response = await fetch('/path/to/song.mp3');
  const audioData = new Uint8Array(await response.arrayBuffer());
  
  // Create decoder
  const decoder = await JsAudioDecoder.create(audioData, 'song.mp3');
  const probe = decoder.getFormat();
  const format = probe.format();
  const durationMs = probe.durationMs();
  
  console.log(`Format: ${format.sampleRate()}Hz, ${format.channels()} channels`);
  console.log(`Duration: ${durationMs !== undefined ? (Number(durationMs) / 1000).toFixed(2) : 'unknown'}s`);
  
  // Create Web Audio context
  const audioContext = new AudioContext();
  
  // Decode all audio
  console.log('Decoding entire file...');
  const allSamples: number[] = [];
  
  while (true) {
    // Decode in large batches for performance
    const samples = await decoder.decodeBatch(44100, 50); // ~1 second
    
    if (samples === null) {
      break; // End of stream
    }
    
    allSamples.push(...(Array.from(samples) as number[]));
  }
  
  console.log(`Decoded ${allSamples.length} total samples`);
  
  // Create AudioBuffer
  const frameCount = allSamples.length / format.channels();
  const audioBuffer = audioContext.createBuffer(
    format.channels(),
    frameCount,
    format.sampleRate()
  );
  
  // De-interleave samples into separate channels
  for (let channel = 0; channel < format.channels(); channel++) {
    const channelData = audioBuffer.getChannelData(channel);
    for (let i = 0; i < frameCount; i++) {
      channelData[i] = allSamples[i * format.channels() + channel];
    }
  }
  
  // Play
  console.log('Playing audio...');
  const source = audioContext.createBufferSource();
  source.buffer = audioBuffer;
  source.connect(audioContext.destination);
  source.start();
  
  console.log('\n✓ Playback started\n');
}

// ============================================================================ 
// 6. Seeking
// ============================================================================ 

async function seekExample() {
  console.log('=== Seeking Example ===\n');
  
  const audioData = await fetch('/path/to/song.mp3')
    .then(res => res.arrayBuffer())
    .then(buf => new Uint8Array(buf));
  
  const decoder = await JsAudioDecoder.create(audioData, 'song.mp3');
  
  // Decode from beginning
  console.log('Decoding from beginning...');
  const samples1 = await decoder.decodeFrames(4096);
  console.log(`Decoded ${samples1?.length || 0} samples`);
  
  // Seek to 30 seconds
  console.log('\nSeeking to 30 seconds...');
  await decoder.seek(30.0);
  
  // Decode from new position
  console.log('Decoding from 30s position...');
  const samples2 = await decoder.decodeFrames(4096);
  console.log(`Decoded ${samples2?.length || 0} samples`);
  
  // Reset to beginning
  console.log('\nResetting to beginning...');
  await decoder.reset();
  
  const samples3 = await decoder.decodeFrames(4096);
  console.log(`Decoded ${samples3?.length || 0} samples`);
  
  console.log('\n✓ Seeking complete\n');
}

// ============================================================================ 
// 7. Format Detection
// ============================================================================ 

async function formatDetection() {
  console.log('=== Format Detection ===\n');
  
  const testFiles = [
    { url: '/test.mp3', name: 'test.mp3' },
    { url: '/test.m4a', name: 'test.m4a' },
    { url: '/test.flac', name: 'test.flac' },
  ];
  
  for (const file of testFiles) {
    try {
      console.log(`\nProbing ${file.name}...`);
      
      const audioData = await fetch(file.url)
        .then(res => res.arrayBuffer())
        .then(buf => new Uint8Array(buf));
      
      const decoder = await JsAudioDecoder.create(audioData, file.name);
      const probe = decoder.getFormat();
      const format = probe.format();
      const durationMs = probe.durationMs();
      
      console.log(`  ✓ Format detected`);
      console.log(`  Sample rate: ${format.sampleRate()} Hz`);
      console.log(`  Channels: ${format.channels()}`);
      console.log(`  Duration: ${durationMs !== undefined ? (Number(durationMs) / 1000).toFixed(2) : 'unknown'}s`);
    } catch (e) {
      console.log(`  ✗ Failed: ${e}`);
    }
  }
  
  console.log('\n✓ Format detection complete\n');
}

// ============================================================================ 
// 8. Offline Cache Manager
// ============================================================================ 

async function offlineCacheExample(library: JsLibrary) {
  console.log('=== Offline Cache Manager ===\n');

  // Bridge dependencies
  const httpClient = new JsHttpClient(null);
  const eventBus = new JsEventBus(256);

  // Google Drive connector (storage provider)
  const storage = JsStorageProviderConfig.googleDrive('oauth-access-token');

  // Configure cache (500 MB encrypted, 4 concurrent downloads)
  const cacheConfig = new JsCacheConfig();
  cacheConfig.setMaxSizeMB(500);
  cacheConfig.setEvictionPolicy('lru');
  cacheConfig.setEncryption(true);
  cacheConfig.setMaxConcurrentDownloads(4);
  cacheConfig.setCacheDirectory('music-cache');

  // Create offline cache manager (namespace isolates filesystem root)
  const manager = await JsOfflineCacheManager.create(
    library,
    cacheConfig,
    'my-app',
    httpClient,
    storage,
    eventBus,
    null // Optional encryption key override
  );

  await manager.initialize();
  console.log('Cache initialized');

  // Kick off a download
  const trackId = 'track_123';
  await manager.downloadTrack(trackId);
  console.log('Download started');

  // Poll cache status
  const status = await manager.cacheStatus(trackId) as JsPlaybackCacheStatus;
  console.log(`Status: ${JsPlaybackCacheStatus[status]}`);

  console.log('\n✓ Offline cache manager ready\n');
}

// ============================================================================ 
// 9. Real-World Example: Streaming Player
// ============================================================================ 

class StreamingPlayer {
  private readonly audioContext = new AudioContext();
  private readonly httpClient = new JsHttpClient(null);
  private session: JsStreamingSession | null = null;
  private ringBuffer: JsRingBuffer | null = null;
  private format: JsAudioFormat | null = null;
  private isPlaying = false;
  private pumpHandle: number | null = null;
  private framesPerPull = 2048;
  private nextStartTime = this.audioContext.currentTime;

  async loadFromRemote(url: string, headers: Record<string, string> = {}) {
    await this.stop();

    const source = JsAudioSource.fromRemote(url, headers);
    const streamingConfig = new JsStreamingConfig();
    streamingConfig.setBufferFrames(44100 * 4); // 4 seconds target buffer
    streamingConfig.setMinBufferFrames(44100);  // 1 second before playback
    streamingConfig.validate();

    const initialRing = new JsRingBuffer(44100 * 6, 2); // Placeholder until session reports actual layout

    this.session = await JsStreamingSession.create(
      source,
      initialRing,
      streamingConfig,
      this.httpClient,
    );

    this.ringBuffer = this.session.ringBuffer();
    const probe = this.session.format();
    this.format = probe.format();

    // Configure chunk size to ~250ms
    this.framesPerPull = Math.floor(this.format.sampleRate() * 0.25);
    this.nextStartTime = this.audioContext.currentTime;

    console.log(`Streaming prepared: ${this.format.sampleRate()}Hz / ${this.format.channels()}ch`);
  }

  async play() {
    if (!this.session || !this.ringBuffer || !this.format) {
      throw new Error('Stream not prepared');
    }

    if (this.isPlaying) {
      return;
    }

    this.isPlaying = true;
    await this.audioContext.resume();
    this.session.start();
    this.schedulePump();
  }

  pause() {
    if (!this.session || !this.isPlaying) {
      return;
    }

    this.isPlaying = false;
    this.session.pause();
    this.cancelPump();
  }

  resume() {
    if (!this.session || this.isPlaying) {
      return;
    }

    this.isPlaying = true;
    this.session.resume();
    this.schedulePump();
  }

  async stop() {
    this.isPlaying = false;
    this.cancelPump();

    if (this.session) {
      this.session.stop();
      await this.session.awaitCompletion();
    }

    this.session = null;
    this.ringBuffer = null;
    this.format = null;
    this.nextStartTime = this.audioContext.currentTime;
  }

  private schedulePump() {
    if (this.pumpHandle !== null) {
      return;
    }

    const step = () => {
      if (!this.isPlaying || !this.session || !this.ringBuffer || !this.format) {
        this.cancelPump();
        return;
      }

      const available = this.ringBuffer.availableFrames();
      if (available > 0) {
        const framesToRead = Math.min(this.framesPerPull, available);
        const samples = this.ringBuffer.readFrames(framesToRead);
        if (samples) {
          this.playSamples(samples);
        }
      } else if (this.session.state() === JsStreamingState.Completed) {
        console.log('Streaming completed');
        this.stop();
        return;
      }

      this.pumpHandle = requestAnimationFrame(step);
    };

    this.pumpHandle = requestAnimationFrame(step);
  }

  private cancelPump() {
    if (this.pumpHandle !== null) {
      cancelAnimationFrame(this.pumpHandle);
      this.pumpHandle = null;
    }
  }

  private playSamples(samples: Float32Array) {
    if (!this.format) {
      return;
    }

    const frames = samples.length / this.format.channels();
    const buffer = this.audioContext.createBuffer(
      this.format.channels(),
      frames,
      this.format.sampleRate(),
    );

    for (let channel = 0; channel < this.format.channels(); channel++) {
      const channelData = buffer.getChannelData(channel);
      for (let i = 0; i < frames; i++) {
        channelData[i] = samples[i * this.format.channels() + channel];
      }
    }

    const source = this.audioContext.createBufferSource();
    source.buffer = buffer;
    source.connect(this.audioContext.destination);

    const startTime = Math.max(this.audioContext.currentTime, this.nextStartTime);
    source.start(startTime);
    this.nextStartTime = startTime + frames / this.format.sampleRate();
  }

  stats(): JsStreamingStats | null {
    return this.session ? this.session.stats() : null;
  }
}

// Usage:
async function streamingPlayerExample() {
  console.log('=== Streaming Player (StreamingService) ===\n');

  const player = new StreamingPlayer();
  await player.loadFromRemote('https://example.com/audio/song.mp3');
  await player.play();

  // Pause after 15 seconds and resume after 3
  setTimeout(() => {
    console.log('Pausing playback...');
    player.pause();

    setTimeout(() => {
      console.log('Resuming playback...');
      player.resume();
    }, 3000);
  }, 15000);
}

// ============================================================================ 
// 10. Performance Tips
// ============================================================================ 

function performanceTips() {
  console.log('=== Performance Tips ===\n');
  
  console.log('1. Use batch decoding for better performance:');
  console.log('   ✓ decodeBatch(44100, 50) - Decode ~1 second in one call');
  console.log('   ✗ decodeFrames(4096) in loop - 40x more boundary crossings\n');
  
  console.log('2. Chunk sizes:');
  console.log('   ✓ 4096-8192 frames for real-time streaming');
  console.log('   ✓ 44100+ frames for pre-decoding\n');
  
  console.log('3. Format hints:');
  console.log('   ✓ Always provide filename with extension');
  console.log('   ✓ Helps decoder probe faster\n');
  
  console.log('4. Web Audio API:');
  console.log('   ✓ Reuse AudioContext (one per app)');
  console.log('   ✓ De-interleave samples once\n');
  
  console.log('5. Memory:');
  console.log('   ✓ Dispose decoders when done');
  console.log('   ✓ Use streaming for large files\n');
}

// ============================================================================ 
// Main
// ============================================================================ 

async function main() {
  try {
    await initializeWasm();
    
    checkFeatures();
    
    // Run examples (comment out as needed)
    // await basicDecoding();
    // await batchDecoding();
    // await decodeAndPlay();
    // await seekExample();
    // await formatDetection();
    // const library = await JsLibrary.create('indexeddb://music-core');
    // await offlineCacheExample(library as JsLibrary);
    // await streamingPlayerExample();
    
    performanceTips();
    
    console.log('✅ All examples completed');
  } catch (error) {
    console.error('❌ Error:', error);
  }
}

// Run if in browser
if (typeof window !== 'undefined') {
  main();
}

// Export for use in other modules
export {
  initializeWasm,
  checkFeatures,
  basicDecoding,
  batchDecoding,
  decodeAndPlay,
  seekExample,
  formatDetection,
  offlineCacheExample,
  StreamingPlayer,
  performanceTips,
};
