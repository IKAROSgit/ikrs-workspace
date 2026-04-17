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
    if (!clientId) {
      setSignInError(
        "VITE_GOOGLE_OAUTH_CLIENT_ID is not set. Add it to .env.local and rebuild.",
      );
      return;
    }

    setSignInError(null);
    setSignInInFlight(true);

    // Register listeners BEFORE calling the backend so we never miss a
    // racing `firebase-auth:id-token-ready` event if Google's consent
    // screen returns very fast.
    const unlistenReady = await listen<{ id_token: string }>(
      "firebase-auth:id-token-ready",
      (event) => {
        resolveRef?.(event.payload.id_token);
      },
    );
    const unlistenError = await listen<{ reason: string }>(
      "firebase-auth:error",
      (event) => {
        rejectRef?.(new Error(event.payload.reason));
      },
    );

    let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
    let resolveRef: ((idToken: string) => void) | null = null;
    let rejectRef: ((err: Error) => void) | null = null;

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
      resolveRef = null;
      rejectRef = null;
      setSignInInFlight(false);
    }
  };

  const cancelSignIn = async () => {
    await cancelFirebaseIdentityFlow().catch(() => {});
    setSignInInFlight(false);
  };

  const logOut = async () => {
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
