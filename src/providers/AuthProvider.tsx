import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { onAuthStateChanged, signInWithPopup, signOut, type User } from "firebase/auth";
import { doc, getDoc, setDoc, serverTimestamp } from "firebase/firestore";
import { auth, db, googleProvider } from "@/lib/firebase";
import type { Consultant } from "@/types";

interface AuthContextValue {
  user: User | null;
  consultant: Consultant | null;
  loading: boolean;
  signIn: () => Promise<void>;
  logOut: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [consultant, setConsultant] = useState<Consultant | null>(null);
  const [loading, setLoading] = useState(true);

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
    await signInWithPopup(auth, googleProvider);
  };

  const logOut = async () => {
    await signOut(auth);
    setConsultant(null);
  };

  return (
    <AuthContext.Provider value={{ user, consultant, loading, signIn, logOut }}>
      {children}
    </AuthContext.Provider>
  );
}

export function AuthGate({ children }: { children: ReactNode }) {
  const { user, loading, signIn } = useAuth();

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
        <button
          onClick={signIn}
          className="px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
        >
          Sign in with Google
        </button>
      </div>
    );
  }

  return <>{children}</>;
}
