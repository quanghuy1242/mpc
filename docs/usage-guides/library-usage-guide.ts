/**
 * TypeScript Usage Guide for core-library WASM
 * Based on ACTUAL generated TypeScript definitions
 * 
 * - JsTrack: new JsTrack(title, provider_id, provider_file_id, duration_ms, disc_number)
 *   + setAlbumId(), setArtistId(), setGenre(), setYear(), etc.
 * - JsAlbum: new JsAlbum(name, artist_id?)
 * - JsArtist: new JsArtist(name)
 * - JsQueryService: JsQueryService.fromLibrary(library)
 * - JsTrackFilter: new JsTrackFilter()
 * 
 * Requires: bridge-wasm/bridge-db.js (SQL.js + IndexedDB bridge)
 */

import init, {
  JsLibrary,
  JsTrack,
  JsAlbum,
  JsArtist,
  JsQueryService,
  JsTrackFilter,
  JsAlbumFilter,
  JsPageRequest,
  name,
  version,
} from '../../core-library/pkg/core_library.js';

// ============================================================================
// 1. SQL.js Bridge Setup (REQUIRED!)
// ============================================================================

/**
 * Before using the library, you MUST load bridge-db.js
 * 
 * Location: bridge-wasm/bridge-db.js
 * 
 * In HTML:
 *   <script src="./bridge-db.js"></script>
 *   <script type="module" src="./app.js"></script>
 * 
 * This provides the global bridgeWasmDb namespace that connects
 * WASM to SQL.js + IndexedDB.
 */

// ============================================================================
// 2. Initialize WASM
// ============================================================================

async function initializeWasm(): Promise<void> {
  await init();
  console.log(`Initialized ${name()} v${version()}`);
}

// ============================================================================
// 3. Create Database
// ============================================================================

async function setupDatabase(): Promise<JsLibrary> {
  const library = await JsLibrary.create("indexeddb://music");
  await library.initialize();
  return library;
}

// ============================================================================
// 4. Working with Tracks (NOW HAS CONSTRUCTOR!)
// ============================================================================

async function createTrack(library: JsLibrary): Promise<string> {
  // ✅ JsTrack NOW has a constructor!
  // Constructor takes required fields, use setters for optional fields
  
  const track = new JsTrack(
    "My Song",              // title
    "google_drive",         // provider_id
    "file_123",             // provider_file_id
    BigInt(240000),         // duration_ms (must be bigint)
    1                       // disc_number
  );
  
  // Set optional fields using setters
  track.setAlbumId("album_1");
  track.setArtistId("artist_1");
  track.setTrackNumber(5);
  track.setGenre("Rock");
  track.setYear(2024);
  track.setBitrate(320);
  track.setSampleRate(44100);
  track.setChannels(2);
  track.setFormat("MP3");
  track.setFileSize(BigInt(9600000));
  track.setMimeType("audio/mpeg");
  
  track.validate();
  await library.insertTrack(track);
  
  return track.id();
}

async function listTracks(library: JsLibrary): Promise<void> {
  const page = new JsPageRequest(0, 20);
  const results = await library.listTracks(page);
  
  console.log(`Total: ${results.total}`);
  for (const track of results.items) {
    console.log(`- ${track.title()}`);
  }
}

// ============================================================================
// 5. Working with Albums (HAS constructor!)
// ============================================================================

async function createAlbum(library: JsLibrary): Promise<string> {
  // JsAlbum HAS a public constructor
  const album = new JsAlbum("Abbey Road", "artist_1");
  album.validate();
  await library.insertAlbum(album);
  return album.id();
}

// ============================================================================
// 6. Working with Artists (HAS constructor!)
// ============================================================================

async function createArtist(library: JsLibrary): Promise<string> {
  // JsArtist HAS a public constructor
  const artist = new JsArtist("The Beatles");
  artist.validate();
  await library.insertArtist(artist);
  return artist.id();
}

// ============================================================================
// 7. Advanced Queries (NOW AVAILABLE after ungating!)
// ============================================================================

async function queryWithFilters(library: JsLibrary): Promise<void> {
  // Use static method (NO constructor!)
  const queryService = JsQueryService.fromLibrary(library);
  
  // JsTrackFilter HAS constructor
  const filter = new JsTrackFilter();
  filter.setGenre("Rock");
  filter.setYear(2024);  // Note: setYear, not setMinYear/setMaxYear
  filter.setMinDurationMs(180000);
  filter.setMaxDurationMs(300000);
  
  const page = new JsPageRequest(0, 20);
  const results = await queryService.queryTracks(filter, page);
  
  console.log(`Found ${results.total} tracks`);
}

async function fullTextSearch(library: JsLibrary, query: string): Promise<void> {
  const queryService = JsQueryService.fromLibrary(library);
  const results = await queryService.search(query);
  
  console.log(`Tracks: ${results.tracks.length}`);
  console.log(`Albums: ${results.albums.length}`);
  console.log(`Artists: ${results.artists.length}`);
}

// ============================================================================
// 8. Complete Example
// ============================================================================

async function main() {
  // 1. Initialize
  await initializeWasm();
  
  // 2. Create database
  const library = await setupDatabase();
  
  // 3. Create artist (has constructor)
  const artist = new JsArtist("The Beatles");
  await library.insertArtist(artist);
  
  // 4. Create album (has constructor)
  const album = new JsAlbum("Abbey Road", artist.id());
  await library.insertAlbum(album);

  // 5. Create track (NOW has constructor!)
  const track = new JsTrack(
    "Come Together",        // title
    "gdrive",               // provider_id
    "f1",                   // provider_file_id
    BigInt(259000),         // duration_ms (must be bigint)
    1                       // disc_number
  );
  
  // Set optional metadata
  track.setAlbumId(album.id());
  track.setArtistId(artist.id());
  track.setTrackNumber(1);
  track.setGenre("Rock");
  track.setYear(1969);
  track.setFormat("MP3");
  
  await library.insertTrack(track);
  
  // 6. Query (now works!)
  const queryService = JsQueryService.fromLibrary(library);
  const results = await queryService.search("Beatles");
  console.log(`Found ${results.tracks.length} tracks`);
}

// ============================================================================
// 9. HTML Setup Example
// ============================================================================

/*
<!DOCTYPE html>
<html>
<head>
  <title>Music Library</title>
</head>
<body>
  <!-- STEP 1: Load bridge-db.js FIRST -->
  <script src="../../bridge-wasm/bridge-db.js"></script>
  
  <!-- STEP 2: Load WASM module -->
  <script type="module">
    import init, { 
      JsLibrary, 
      JsTrack,
      JsAlbum,
      JsArtist,
      JsQueryService 
    } from './core_library.js';
    
    async function main() {
      await init();
      const library = await JsLibrary.create("indexeddb://demo");
      await library.initialize();
      
      // Create artist (HAS constructor)
      const artist = new JsArtist("Test Artist");
      await library.insertArtist(artist);
      
      // Create album (HAS constructor)
      const album = new JsAlbum("Test Album", artist.id());
      await library.insertAlbum(album);
      
      // Create track (NOW has constructor!)
      const track = new JsTrack("Test Track", "test", "f1", BigInt(180000), 1);
      track.setAlbumId(album.id());
      track.setArtistId(artist.id());
      track.setFormat("MP3");
      
      await library.insertTrack(track);
      
      // Query (now available!)
      const queryService = JsQueryService.fromLibrary(library);
      const results = await queryService.search("test");
      
      console.log(`Found ${results.tracks.length} tracks`);
    }
    
    main();
  </script>
</body>
</html>
*/

// ============================================================================
// KEY POINTS
// ============================================================================

/**
 * ✅ CONSISTENT API ACHIEVED!
 * 
 * 1. Query module is NOW UNGATED - JsQueryService works in WASM!
 * 2. ALL models NOW have constructors:
 *    - JsTrack: new JsTrack(title, provider_id, file_id, duration, disc)
 *    - JsAlbum: new JsAlbum(name, artist_id?)
 *    - JsArtist: new JsArtist(name)
 * 3. JsTrack has SETTERS for optional fields (builder pattern):
 *    - setAlbumId(), setArtistId(), setTrackNumber()
 *    - setGenre(), setYear(), setBitrate(), etc.
 * 4. JsQueryService: use JsQueryService.fromLibrary(library)
 * 5. bridge-db.js location: bridge-wasm/bridge-db.js
 * 6. Load bridge-db.js BEFORE the WASM module
 * 
 * Alternative: JsTrack.fromObject() still works for complex initialization
 */

export {
  initializeWasm,
  setupDatabase,
  createTrack,
  createAlbum,
  createArtist,
  listTracks,
  queryWithFilters,
  fullTextSearch,
  main,
};
