import { useMutation, useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";
import { spenderApi } from "./api";

export const useStatus = () =>
  useQuery({ queryKey: ["spender", "status"], queryFn: spenderApi.datasetStatus });

export const useCurrencies = (enabled: boolean) =>
  useQuery({
    queryKey: ["spender", "currencies"],
    queryFn: spenderApi.currencies,
    enabled,
    staleTime: Infinity,
  });

const STALE = 5 * 60_000;

export const offersKey = (currency: number | null, world: string) => [
  "spender",
  "offers",
  currency,
  world,
];
export const useOffers = (currency: number | null, world: string) =>
  useQuery({
    queryKey: offersKey(currency, world),
    queryFn: () => spenderApi.offers(currency ?? 0, world, false),
    enabled: currency !== null && world !== "",
    staleTime: STALE,
  });

export const gilOffersKey = (world: string) => ["spender", "gilOffers", world];
export const useGilOffers = (world: string) =>
  useQuery({
    queryKey: gilOffersKey(world),
    enabled: world !== "",
    queryFn: () => spenderApi.gilOffers(world, false),
    staleTime: STALE,
  });

export const deliveriesKey = (world: string, seal: number) => [
  "spender",
  "deliveries",
  world,
  seal,
];
export const useDeliveries = (world: string, seal: number) =>
  useQuery({
    queryKey: deliveriesKey(world, seal),
    enabled: world !== "",
    queryFn: () => spenderApi.deliveries(world, seal, false),
    staleTime: STALE,
  });

export const venturesKey = (world: string) => ["spender", "ventures", world];
export const useVentures = (world: string) =>
  useQuery({
    queryKey: venturesKey(world),
    enabled: world !== "",
    queryFn: () => spenderApi.ventures(world, false),
    staleTime: STALE,
  });

export const desynthsKey = (world: string) => ["spender", "desynths", world];
export const useDesynths = (world: string) =>
  useQuery({
    queryKey: desynthsKey(world),
    enabled: world !== "",
    queryFn: () => spenderApi.desynths(world, false),
    staleTime: STALE,
  });

/** Relit les prix sans attendre l'expiration du cache côté Rust. */
export function useRefresh<T>(key: QueryKey, fetcher: () => Promise<T>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fetcher,
    onSuccess: (data) => {
      qc.setQueryData(key, data);
    },
  });
}

export function useBuildDataset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: spenderApi.buildDataset,
    onSuccess: (status) => {
      qc.setQueryData(["spender", "status"], status);
      void qc.invalidateQueries({ queryKey: ["spender", "currencies"] });
      void qc.invalidateQueries({ queryKey: ["spender", "offers"] });
      void qc.invalidateQueries({ queryKey: ["spender", "gilOffers"] });
      void qc.invalidateQueries({ queryKey: ["spender", "deliveries"] });
      void qc.invalidateQueries({ queryKey: ["spender", "ventures"] });
      void qc.invalidateQueries({ queryKey: ["spender", "desynths"] });
    },
  });
}
