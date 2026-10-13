import { useEffect, useState } from "react";
import { Loader2Icon, SearchIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { EmptyState, PageHeader } from "@/components/Page";
import { ItemIcon } from "@/components/market";
import { errorMessage } from "@/lib/api";
import { useSearch } from "@/lib/queries";
import { useNav } from "@/store/nav";

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => {
      setDebounced(value);
    }, ms);
    return () => {
      clearTimeout(t);
    };
  }, [value, ms]);
  return debounced;
}

export function SearchPage() {
  const [text, setText] = useState("");
  const query = useDebounced(text.trim(), 300);
  const search = useSearch(query);
  const openItem = useNav((s) => s.openItem);
  const results = query.length >= 2 ? (search.data ?? []) : [];

  return (
    <>
      <PageHeader title="Recherche" />
      <div className="flex items-center gap-2 border-b px-4 py-2">
        <SearchIcon className="size-3.5 text-muted-foreground" />
        <Input
          autoFocus
          placeholder="Nom de l'objet (2 caractères min.)"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
          }}
          className="h-7 max-w-md border-0 shadow-none focus-visible:ring-0"
        />
        {search.isFetching && (
          <Loader2Icon className="size-3.5 animate-spin text-muted-foreground" />
        )}
      </div>
      {search.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(search.error)}
          <Button variant="outline" size="sm" onClick={() => void search.refetch()}>
            Réessayer
          </Button>
        </div>
      ) : query.length < 2 ? (
        <EmptyState message="Tape le nom d'un objet pour le rechercher." />
      ) : search.isPending ? (
        <div className="flex flex-col gap-1 p-4">
          {Array.from({ length: 8 }, (_, i) => (
            <Skeleton key={i} className="h-8" />
          ))}
        </div>
      ) : results.length === 0 ? (
        <EmptyState message={`Aucun objet vendable à l'HV ne correspond à « ${query} ».`} />
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto py-1">
          {results.map((hit) => (
            <li key={hit.id}>
              <button
                type="button"
                onClick={() => {
                  openItem(hit.id);
                }}
                className="flex h-9 w-full items-center gap-2.5 px-4 text-left text-xs hover:bg-muted/50"
              >
                <ItemIcon path={hit.iconPath} />
                <span className="truncate">{hit.name}</span>
                <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
                  {hit.category}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
