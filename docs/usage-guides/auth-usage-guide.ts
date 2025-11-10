/**
 * TypeScript Usage Guide for core-auth WASM
 *
 * OAuth 2.0 Authentication for Cloud Storage Providers
 *
 * Features:
 * - Google Drive and OneDrive OAuth 2.0 flows
 * - PKCE security for public clients
 * - Type-safe provider and state enums
 * - Profile ID management
 *
 * Note: Full AuthManager requires platform bridges (not yet exposed).
 * This guide demonstrates the type system and utility functions.
 */

import init,
  {
    // Main Class
    JsAuthManager,

    // Enums
    JsProviderKind,
    JsAuthState,

    // Types
    JsProviderInfo,

    // Utilities
    authVersion,
    authName,
    parseProvider,
    providerDisplayName,
    isValidProfileId,
  } from '../../core-auth/pkg/core_auth.js';

// Import dependencies from other modules
import { JsEventBus } from '../../core-runtime/pkg/core_runtime.js';
import { JsHttpClient, JsSecureStore } from '../../bridge-wasm/pkg/bridge_wasm.js';

// ============================================================================ 
// 1. Initialize WASM
// ============================================================================ 

async function initializeWasm() {
  console.log('=== Initializing core-auth WASM ===\n');
  
  // Step 1: Initialize WASM binary
  await init();
  
  console.log(`Module: ${authName()}`);
  console.log(`Version: ${authVersion()}`);
  
  // Step 2: Create bridges (your custom implementations!)
  const eventBus = new JsEventBus(100);
  console.log('✓ Event bus created');
  
  const httpClient = new JsHttpClient(); // Or: new JsHttpClient(customFetch)
  console.log('✓ HTTP client created');
  
  const secureStore = new JsSecureStore("auth");
  console.log('✓ Secure store created');
  
  // Step 3: Create AuthManager with bridges (beautiful API!)
  const authManager = new JsAuthManager(eventBus, httpClient, secureStore);
  console.log('✓ AuthManager created\n');
  
  return { eventBus, httpClient, secureStore, authManager };
}

// ============================================================================ 
// 2. Provider Information
// ============================================================================ 

function providerExamples() {
  console.log('=== Provider Examples ===\n');
  
  // Use enum values
  const googleDrive = JsProviderKind.GoogleDrive;
  const oneDrive = JsProviderKind.OneDrive;
  
  console.log(`Google Drive: ${providerDisplayName(googleDrive)}`);
  console.log(`OneDrive: ${providerDisplayName(oneDrive)}`);
  
  // Parse from string
  const parsed = parseProvider('google_drive');
  if (parsed !== undefined) {
    console.log(`Parsed provider: ${providerDisplayName(parsed)}`);
  }
  
  console.log('\n✓ Provider examples complete\n');
}

// ============================================================================ 
// 3. Authentication State
// ============================================================================ 

function authStateExamples() {
  console.log('=== Auth State Examples ===\n');
  
  // State enum values
  const signedOut = JsAuthState.SignedOut;
  const signingIn = JsAuthState.SigningIn;
  const signedIn = JsAuthState.SignedIn;
  const refreshing = JsAuthState.TokenRefreshing;
  
  console.log('Available states:');
  console.log(`  - SignedOut: ${signedOut}`);
  console.log(`  - SigningIn: ${signingIn}`);
  console.log(`  - SignedIn: ${signedIn}`);
  console.log(`  - TokenRefreshing: ${refreshing}`);
  
  console.log('\n✓ Auth state examples complete\n');
}

// ============================================================================ 
// 4. Profile ID Management
// ============================================================================ 

function profileIdExamples() {
  console.log('=== Profile ID Examples ===\n');
  
  // Validate profile IDs
  const validId = '550e8400-e29b-41d4-a716-446655440000';
  const invalidId = 'not-a-uuid';
  
  console.log(`Is '${validId}' valid? ${isValidProfileId(validId)}`);
  console.log(`Is '${invalidId}' valid? ${isValidProfileId(invalidId)}`);
  
  // In a real app, profile IDs are generated server-side
  // and returned from completeSignIn()
  
  console.log('\n✓ Profile ID examples complete\n');
}

// ============================================================================ 
// 5. OAuth Flow Pattern (Conceptual)
// ============================================================================ 

/**
 * Complete OAuth Flow - REAL WORKING EXAMPLE
 */

async function realOAuthFlow() {
  console.log('=== Real OAuth Flow Example ===\n');
  
  try {
    // Create bridges and AuthManager (your beautiful API!)
    const eventBus = new JsEventBus(100);
    const httpClient = new JsHttpClient(); // Could pass custom fetch here!
    const secureStore = new JsSecureStore("auth");
    const authManager = new JsAuthManager(eventBus, httpClient, secureStore);
    
    // Step 1: List available providers
    const providers = authManager.listProviders();
    console.log('Available providers:', providers.map((p: any) => p.displayName));
    
    // Step 2: Start sign-in
    const authUrl = await authManager.signIn(JsProviderKind.GoogleDrive);
    console.log('Auth URL generated:', authUrl);
    console.log('→ Open this in browser/popup');
    
    // In real app:
    // window.open(authUrl, 'oauth', 'width=600,height=800');
    
    // Step 3: Handle OAuth callback (in callback page)
    // const urlParams = new URLSearchParams(window.location.search);
    // const code = urlParams.get('code')!;
    // const state = urlParams.get('state')!;
    
    // Step 4: Complete sign-in
    // const profileId = await authManager.completeSignIn(
    //   JsProviderKind.GoogleDrive,
    //   code,
    //   state
    // );
    // console.log('Signed in! Profile ID:', profileId);
    
    // Step 5: Store profile ID
    // localStorage.setItem('currentProfileId', profileId);
    
    // Step 6: Sign out later
    // await authManager.signOut(profileId);
    
    console.log('\n✓ OAuth flow demonstrated (commented out for safety)\n');
  } catch (error) {
    console.error('OAuth error:', error);
  }
}

// ============================================================================ 
// 6. JavaScript OAuth Implementation (Current Workaround)
// ============================================================================ 

/**
 * Since full AuthManager isn't exported yet, implement OAuth in TypeScript.
 * Use core-auth types for type safety.
 */

class SimpleOAuthClient {
  private provider: typeof JsProviderKind.GoogleDrive | typeof JsProviderKind.OneDrive;
  private clientId: string;
  private redirectUri: string;
  
  constructor(
    provider: typeof JsProviderKind.GoogleDrive | typeof JsProviderKind.OneDrive,
    clientId: string,
    redirectUri: string
  ) {
    this.provider = provider;
    this.clientId = clientId;
    this.redirectUri = redirectUri;
  }
  
  /**
   * Generate PKCE challenge
   */
  private async generatePKCE(): Promise<{ verifier: string; challenge: string }> {
    const array = new Uint8Array(32);
    crypto.getRandomValues(array);
    const verifier = btoa(String.fromCharCode(...array))
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=/g, '');
    
    const encoder = new TextEncoder();
    const data = encoder.encode(verifier);
    const hash = await crypto.subtle.digest('SHA-256', data);
    const challenge = btoa(String.fromCharCode(...new Uint8Array(hash)))
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=/g, '');
    
    return { verifier, challenge };
  }
  
  /**
   * Start OAuth flow
   */
  async startFlow(): Promise<string> {
    const { verifier, challenge } = await this.generatePKCE();
    const state = crypto.randomUUID();
    
    // Store for later
    sessionStorage.setItem('oauth_verifier', verifier);
    sessionStorage.setItem('oauth_state', state);
    
    // Build auth URL based on provider
    const params = new URLSearchParams({
      client_id: this.clientId,
      redirect_uri: this.redirectUri,
      response_type: 'code',
      state,
      code_challenge: challenge,
      code_challenge_method: 'S256',
    });
    
    let authUrl: string;
    if (this.provider === JsProviderKind.GoogleDrive) {
      params.append('scope', 'https://www.googleapis.com/auth/drive.readonly');
      authUrl = `https://accounts.google.com/o/oauth2/v2/auth?${params}`;
    } else {
      params.append('scope', 'Files.Read offline_access');
      authUrl = `https://login.microsoftonline.com/common/oauth2/v2.0/authorize?${params}`;
    }
    
    return authUrl;
  }
  
  /**
   * Handle callback and exchange code for token
   */
  async handleCallback(code: string, state: string): Promise<string> {
    // Verify state
    const savedState = sessionStorage.getItem('oauth_state');
    if (state !== savedState) {
      throw new Error('State mismatch - possible CSRF attack');
    }
    
    const verifier = sessionStorage.getItem('oauth_verifier');
    if (!verifier) {
      throw new Error('No verifier found');
    }
    
    // Exchange code for token
    const tokenUrl = this.provider === JsProviderKind.GoogleDrive
      ? 'https://oauth2.googleapis.com/token'
      : 'https://login.microsoftonline.com/common/oauth2/v2.0/token';
    
    const response = await fetch(tokenUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        client_id: this.clientId,
        code,
        redirect_uri: this.redirectUri,
        grant_type: 'authorization_code',
        code_verifier: verifier,
      }),
    });
    
    if (!response.ok) {
      throw new Error(`Token exchange failed: ${response.statusText}`);
    }
    
    const tokens = await response.json();
    
    // Clean up session
    sessionStorage.removeItem('oauth_verifier');
    sessionStorage.removeItem('oauth_state');
    
    return tokens.access_token;
  }
}

// Usage example
async function oauthExample() {
  console.log('=== OAuth Flow Example ===\n');
  
  // This is a PLACEHOLDER - use your actual client ID
  const client = new SimpleOAuthClient(
    JsProviderKind.GoogleDrive,
    'YOUR_CLIENT_ID.apps.googleusercontent.com',
    'http://localhost:3000/callback'
  );
  
  // Step 1: Start flow (in button click handler)
  // const authUrl = await client.startFlow();
  // window.location.href = authUrl;
  
  // Step 2: Handle callback (in callback page)
  // const urlParams = new URLSearchParams(window.location.search);
  // const code = urlParams.get('code')!;
  // const state = urlParams.get('state')!;
  // const token = await client.handleCallback(code, state);
  // console.log('Access token:', token);
  
  console.log('See implementation above for complete flow\n');
}

// ============================================================================ 
// 7. Type Reference
// ============================================================================ 

function typeReference() {
  console.log('=== Type Reference ===\n');
  
  console.log('Exported Types:');
  console.log('  - JsProviderKind: GoogleDrive | OneDrive');
  console.log('  - JsAuthState: SignedOut | SigningIn | SignedIn | TokenRefreshing');
  console.log('  - JsProviderInfo: { kind, displayName, authUrl, tokenUrl, scopes }');
  console.log('');
  console.log('Exported Functions:');
  console.log('  - authVersion(): string');
  console.log('  - authName(): string');
  console.log('  - parseProvider(s: string): JsProviderKind | undefined');
  console.log('  - providerDisplayName(provider: JsProviderKind): string');
  console.log('  - isValidProfileId(s: string): boolean');
  
  console.log('\n✓ Type reference complete\n');
}

// ============================================================================ 
// 8. Integration with core-runtime Events
// ============================================================================ 

/**
 * core-auth emits events via core-runtime's EventBus.
 * Listen for auth events to update UI.
 */

function eventIntegration() {
  console.log('=== Event Integration ===\n');
  
  // Example: Listen for auth events (requires core-runtime EventBus)
  // const eventBus = new EventBus(100);
  // 
  // eventBus.subscribe((event) => {
  //   if (event.type === 'Auth') {
  //     switch (event.variant) {
  //       case 'SigningIn':
  //         console.log('User is signing in...');
  //         break;
  //       case 'SignedIn':
  //         console.log('User signed in:', event.profile_id);
  //         break;
  //       case 'SignedOut':
  //         console.log('User signed out');
  //         break;
  //     }
  //   }
  // });
  
  console.log('Auth events integrate with core-runtime EventBus\n');
}

// ============================================================================ 
// Main
// ============================================================================ 

async function main() {
  try {
    const { eventBus, httpClient, secureStore, authManager } = await initializeWasm();
    
    providerExamples();
    authStateExamples();
    profileIdExamples();
    await realOAuthFlow(); // Now uses the BEAUTIFUL API!
    await oauthExample(); // JavaScript fallback still available
    typeReference();
    eventIntegration();
    
    console.log('✅ All examples completed');
    console.log('\n🎉 YOUR BEAUTIFUL API IS LIVE!');
    console.log('✅ new JsAuthManager(eventBus, httpClient, secureStore)');
    console.log('✅ Pass custom fetch: new JsHttpClient(customFetch)');
    console.log('✅ Symmetric API between Rust and JavaScript!\n');
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
  providerExamples,
  authStateExamples,
  profileIdExamples,
  oauthExample,
  SimpleOAuthClient,
  typeReference,
  eventIntegration,
};
