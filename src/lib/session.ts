// Compte connecté (client web).
import { serverCall } from "@/lib/backend";

let current: string | null = null;

/** Identifiant du compte connecté, `null` dans l'app de bureau. */
export const username = () => current;

export function setUsername(name: string) {
  current = name;
}

export async function logout() {
  await serverCall("/api/auth/logout").catch(() => undefined);
  location.reload();
}
