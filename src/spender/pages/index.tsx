import { useEffect } from "react";
import { EmptyState } from "@/components/Page";
import { CurrencySidebar } from "@/spender/CurrencySidebar";
import { SpenderGate } from "@/spender/SpenderGate";
import { DeliveriesPage } from "@/spender/pages/DeliveriesPage";
import { DesynthPage } from "@/spender/pages/DesynthPage";
import { GilShopPage } from "@/spender/pages/GilShopPage";
import { MonnaiesPage } from "@/spender/pages/MonnaiesPage";
import { VenturesPage } from "@/spender/pages/VenturesPage";
import { useCurrencies, useStatus } from "@/spender/queries";
import { usePrefs } from "@/spender/prefs";

function Monnaies() {
  const status = useStatus();
  const currencies = useCurrencies(status.data?.ready === true);
  const { currency, set } = usePrefs();

  // Première monnaie par défaut, ou si celle mémorisée n'existe plus.
  useEffect(() => {
    const list = currencies.data ?? [];
    const first = list[0];
    if (first && !list.some((c) => c.itemId === currency)) {
      set({ currency: first.itemId });
    }
  }, [currencies.data, currency, set]);

  const selected = currencies.data?.find((c) => c.itemId === currency) ?? null;
  return (
    <div className="flex min-h-0 flex-1">
      <CurrencySidebar
        currencies={currencies.data ?? []}
        selected={currency}
        onSelect={(c) => {
          set({ currency: c });
        }}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        {selected ? (
          <MonnaiesPage key={selected.itemId} currency={selected} />
        ) : (
          <EmptyState message="Choisis une monnaie à gauche." />
        )}
      </div>
    </div>
  );
}

export function MonnaiesRoute() {
  return (
    <SpenderGate>
      <Monnaies />
    </SpenderGate>
  );
}

export function GilShopRoute() {
  return (
    <SpenderGate>
      <GilShopPage />
    </SpenderGate>
  );
}

export function DeliveriesRoute() {
  return (
    <SpenderGate>
      <DeliveriesPage />
    </SpenderGate>
  );
}

export function VenturesRoute() {
  return (
    <SpenderGate>
      <VenturesPage />
    </SpenderGate>
  );
}

export function DesynthRoute() {
  return (
    <SpenderGate>
      <DesynthPage />
    </SpenderGate>
  );
}
