import { HomeIcon, NotebookPenIcon, SettingsIcon, ShoppingCartIcon, TagIcon } from "lucide-react";
import type { Page } from "@/store/nav";

/** Pages du mode compagnon (téléphone), dans l'ordre de la barre du bas. */
export const COMPANION_PAGES: { page: Page; label: string; icon: typeof HomeIcon }[] = [
  { page: "dashboard", label: "Accueil", icon: HomeIcon },
  { page: "ventes", label: "Ventes", icon: TagIcon },
  { page: "courses", label: "Courses", icon: ShoppingCartIcon },
  { page: "journal", label: "Journal", icon: NotebookPenIcon },
  { page: "settings", label: "Réglages", icon: SettingsIcon },
];
