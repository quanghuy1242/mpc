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
  JsCacheStatus,
  
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
} from '../core-playback/pkg/core_playback.js';

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
// 8. Cache Configuration
// ============================================================================

function cacheConfiguration() {
  console.log('=== Cache Configuration ===\n');
  
  const cacheConfig = new JsCacheConfig();
  
  // Set max cache size (in MB)
  cacheConfig.setMaxSizeMB(500); // 500 MB
  
  // Set eviction policy
  cacheConfig.setEvictionPolicy('lru'); // or 'lfu', 'fifo', 'largest_first'
  
  // Enable encryption
  cacheConfig.setEncryption(true);
  
  // Set max concurrent downloads
  cacheConfig.setMaxConcurrentDownloads(4);
  
  // Set cache directory
  cacheConfig.setCacheDirectory('/data/music-cache');
  
  console.log('Cache configuration:');
  console.log('  Max size: 500 MB');
  console.log('  Policy: LRU');
  console.log('  Encryption: enabled');
  console.log('  Max concurrent downloads: 4');
  
  console.log('\n✓ Cache configured\n');
  
  // Note: Actual cache manager (OfflineCacheManager) not yet exposed to JS
  // This will be added in a future iteration
}

// ============================================================================
// 9. Real-World Example: Streaming Player
// ============================================================================

class StreamingPlayer {
  private audioContext: AudioContext;
  private decoder: any = null;
  private currentSource: AudioBufferSourceNode | null = null;
  private isPlaying = false;
  
  constructor() {
    this.audioContext = new AudioContext();
  }
  
  async loadTrack(url: string) {
    console.log(`Loading track: ${url}`);
    
    // Fetch audio data
    const response = await fetch(url);
    const audioData = new Uint8Array(await response.arrayBuffer());
    
    // Create decoder
    const filename = url.split('/').pop() || 'audio';
    this.decoder = await JsAudioDecoder.create(audioData, filename);
    
    // Get format info
    const probe = this.decoder.getFormat();
    const format = probe.format();
    const durationMs = probe.durationMs();
    console.log(`Loaded: ${format.sampleRate()}Hz, ${format.channels()}ch, ${durationMs !== undefined ? (Number(durationMs) / 1000).toFixed(2) : 'unknown'}s`);
    
    return format;
  }
  
  async play() {
    if (!this.decoder) {
      throw new Error('No track loaded');
    }
    
    if (this.isPlaying) {
      console.log('Already playing');
      return;
    }
    
    this.isPlaying = true;
    
    // Reset decoder
    await this.decoder.reset();
    
    // Start playback loop
    await this.playbackLoop();
  }
  
  async playbackLoop() {
    const probe = this.decoder.getFormat();
    const format = probe.format();
    const chunkDuration = 1.0; // 1 second chunks
    const chunkFrames = Math.floor(format.sampleRate() * chunkDuration);
    
    while (this.isPlaying) {
      // Decode chunk
      const samples = await this.decoder.decodeBatch(chunkFrames, 50);
      
      if (samples === null) {
        // End of track
        this.isPlaying = false;
        console.log('Playback completed');
        break;
      }
      
      // Create and play audio buffer
      await this.playChunk(samples, format);
    }
  }
  
  async playChunk(samples: Float32Array, format: any) {
    const frameCount = samples.length / format.channels();
    const audioBuffer = this.audioContext.createBuffer(
      format.channels(),
      frameCount,
      format.sampleRate()
    );
    
    // De-interleave
    for (let channel = 0; channel < format.channels(); channel++) {
      const channelData = audioBuffer.getChannelData(channel);
      for (let i = 0; i < frameCount; i++) {
        channelData[i] = samples[i * format.channels() + channel];
      }
    }
    
    // Play
    const source = this.audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(this.audioContext.destination);
    source.start();
    
    this.currentSource = source;
    
    // Wait for chunk to finish
    await new Promise(resolve => {
      source.onended = resolve;
    });
  }
  
  stop() {
    this.isPlaying = false;
    if (this.currentSource) {
      this.currentSource.stop();
      this.currentSource = null;
    }
  }
  
  async seek(position: number) {
    if (!this.decoder) {
      throw new Error('No track loaded');
    }
    
    const wasPlaying = this.isPlaying;
    this.stop();
    
    await this.decoder.seek(position);
    
    if (wasPlaying) {
      await this.play();
    }
  }
}

// Usage:
async function streamingPlayerExample() {
  console.log('=== Streaming Player Example ===\n');
  
  const player = new StreamingPlayer();
  
  await player.loadTrack('/path/to/song.mp3');
  await player.play();
  
  // Seek after 5 seconds
  setTimeout(async () => {
    console.log('Seeking to 30 seconds...');
    await player.seek(30.0);
  }, 5000);
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
    // cacheConfiguration();
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
  cacheConfiguration,
  StreamingPlayer,
  performanceTips,
};
