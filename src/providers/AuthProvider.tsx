import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import {
  GoogleAuthProvider,
  onAuthStateChanged,
  signInWithCredential,
  signOut,
  type User,
} from "firebase/auth";
import { doc, getDoc, setDoc, serverTimestamp } from "firebase/firestore";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { auth, db } from "@/lib/firebase";
import {
  cancelFirebaseIdentityFlow,
  clearTokenCache,
  startFirebaseIdentityFlow,
} from "@/lib/tauri-commands";
import type { Consultant } from "@/types";

interface AuthContextValue {
  user: User | null;
  consultant: Consultant | null;
  loading: boolean;
  signInInFlight: boolean;
  signInError: string | null;
  signIn: () => Promise<void>;
  cancelSignIn: () => Promise<void>;
  logOut: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// Dedicated port range for the Firebase identity flow. Does not overlap
// the engagement-OAuth port (49152) so both flows can coexist.
const FIREBASE_IDENTITY_PORT = 49153;

// 5-minute cap — Google leaves the consent screen open for the user;
// the flow auto-aborts after this so the UI doesn't wedge indefinitely.
const FIREBASE_SIGNIN_TIMEOUT_MS = 5 * 60 * 1000;

/**
 * Decode the unsigned payload of a JWT without verifying the signature.
 * Safe because we only use it to read the `nonce` claim for equality
 * comparison against a value we generated ourselves. Firebase's
 * signInWithCredential performs the full signature + iss + aud + exp
 * validation when we pass the credential — we do not relitigate that
 * here, only the nonce which Firebase does not validate.
 */
function decodeJwtClaims(jwt: string): { nonce?: string } | null {
  const parts = jwt.split(".");
  if (parts.length < 2) return null;
  const [, payload] = parts;
  if (!payload) return null;
  try {
    // Base64URL → base64 conversion, pad, decode.
    const b64 = payload.replace(/-/g, "+").replace(/_/g, "/");
    const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const json = atob(padded);
    return JSON.parse(json) as { nonce?: string };
  } catch {
    return null;
  }
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [consultant, setConsultant] = useState<Consultant | null>(null);
  const [loading, setLoading] = useState(true);
  const [signInInFlight, setSignInInFlight] = useState(false);
  const [signInError, setSignInError] = useState<string | null>(null);

  useEffect(() => {
    const unsubscribe = onAuthStateChanged(auth, async (firebaseUser) => {
      setUser(firebaseUser);
      if (firebaseUser) {
        const ref = doc(db, "consultants", firebaseUser.uid);
        const snap = await getDoc(ref);
        if (snap.exists()) {
          setConsultant(snap.data() as Consultant);
        } else {
          const newConsultant: Consultant = {
            _v: 1,
            id: firebaseUser.uid,
            email: firebaseUser.email ?? "",
            name: firebaseUser.displayName ?? "",
            role: "consultant",
            preferences: {
              theme: "dark",
              terminal: "default",
              timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
            },
            createdAt: new Date(),
            updatedAt: new Date(),
          };
          await setDoc(ref, {
            ...newConsultant,
            createdAt: serverTimestamp(),
            updatedAt: serverTimestamp(),
          });
          setConsultant(newConsultant);
        }
      } else {
        setConsultant(null);
      }
      setLoading(false);
    });
    return unsubscribe;
  }, []);

  const signIn = async () => {
    if (signInInFlight) return;

    const clientId = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
    const clientSecret = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_SECRET ?? "";
    if (!clientId) {
      setSignInError(
        "VITE_GOOGLE_OAUTH_CLIENT_ID is not set. Add it to .env.local and rebuild.",
      );
      return;
    }
    if (!clientSecret) {
      setSignInError(
        "VITE_GOOGLE_OAUTH_CLIENT_SECRET is not set. Add it to .env.local and rebuild. " +
          "Google Desktop-app OAuth clients require the secret at the token endpoint even with PKCE.",
      );
      return;
    }

    setSignInError(null);
    setSignInInFlight(true);

    // Hoist the promise ref slots ABOVE the listener registrations so
    // TypeScript + a future refactor can never introduce a window where
    // the callback closes over a binding that hasn't been initialised.
    // At runtime they're `undefined` until the Promise constructor runs;
    // the optional-chain call safely no-ops if an event fires before.
    let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
    let resolveRef: ((idToken: string) => void) | undefined;
    let rejectRef: ((err: Error) => void) | undefined;

    // Register listeners BEFORE calling the backend so we never miss a
    // racing `firebase-auth:id-token-ready` event if Google's consent
    // screen returns very fast.
    const unlistenReady = await listen<{ id_token: string; expected_nonce: string }>(
      "firebase-auth:id-token-ready",
      (event) => {
        // Nonce validation (Codex HIGH-2): Firebase's signInWithCredential
        // does NOT auto-verify the `nonce` claim in the id_token against
        // the nonce we embedded in the auth URL. We decode the JWT payload
        // ourselves and compare. A mismatch means either the flow was
        // replayed with a stale token, or Google echoed the wrong nonce
        // (unlikely) — either way, fail closed.
        const claims = decodeJwtClaims(event.payload.id_token);
        if (claims && claims.nonce !== event.payload.expected_nonce) {
          rejectRef?.(
            new Error(
              "Sign-in nonce mismatch — possible replay. Close your browser tab and try again.",
            ),
          );
          return;
        }
        resolveRef?.(event.payload.id_token);
      },
    );
    const unlistenError = await listen<{ reason: string }>(
      "firebase-auth:error",
      (event) => {
        rejectRef?.(new Error(event.payload.reason));
      },
    );

    try {
      const idTokenPromise = new Promise<string>((resolve, reject) => {
        resolveRef = resolve;
        rejectRef = reject;
        timeoutHandle = setTimeout(() => {
          reject(new Error("Sign-in timed out. Try again."));
        }, FIREBASE_SIGNIN_TIMEOUT_MS);
      });

      const { auth_url } = await startFirebaseIdentityFlow(
        clientId,
        clientSecret,
        FIREBASE_IDENTITY_PORT,
      );
      await openUrl(auth_url);

      const idToken = await idTokenPromise;
      const credential = GoogleAuthProvider.credential(idToken);
      await signInWithCredential(auth, credential);
      // onAuthStateChanged picks up the user from here; React state updates follow.
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setSignInError(msg);
      // Fire-and-forget cleanup of any half-started Rust server.
      cancelFirebaseIdentityFlow().catch(() => {});
    } finally {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      unlistenReady();
      unlistenError();
      resolveRef = undefined;
      rejectRef = undefined;
      setSignInInFlight(false);
    }
  };

  const cancelSignIn = async () => {
    await cancelFirebaseIdentityFlow().catch(() => {});
    setSignInInFlight(false);
  };

  const logOut = async () => {
    // Clear the in-memory Google access-token cache BEFORE signing
    // out of Firebase so subsequent sign-ins (potentially from a
    // different consultant on the same Mac session) don't inherit
    // the prior consultant's tokens. Codex 2026-04-18 token-cache
    // review must-fix #3. Failure of the cache-clear is non-blocking
    // — the tokens will naturally expire within an hour anyway.
    clearTokenCache().catch(() => {});
    await signOut(auth);
    setConsultant(null);
  };

  return (
    <AuthContext.Provider
      value={{
        user,
        consultant,
        loading,
        signInInFlight,
        signInError,
        signIn,
        cancelSignIn,
        logOut,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function AuthGate({ children }: { children: ReactNode }) {
  const { user, loading, signIn, signInInFlight, signInError, cancelSignIn } = useAuth();

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen bg-background">
        <p className="text-muted-foreground">Loading...</p>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="flex flex-col items-center justify-center h-screen bg-background gap-6">
        <h1 className="text-2xl font-bold">IKAROS Workspace</h1>
        <p className="text-muted-foreground">Sign in with your IKAROS Google account</p>
        {signInInFlight ? (
          <div className="flex flex-col items-center gap-3">
            <p className="text-sm text-muted-foreground">
              Complete the sign-in in your browser, then return here.
            </p>
            <button
              onClick={cancelSignIn}
              className="px-4 py-1.5 text-sm border rounded-md hover:bg-muted transition-colors"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            onClick={signIn}
            className="px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            Sign in with Google
          </button>
        )}
        {signInError && (
          <p className="text-sm text-destructive max-w-md text-center">{signInError}</p>
        )}
      </div>
    );
  }

  return <>{children}</>;
}
