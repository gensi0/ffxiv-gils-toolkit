import {
  createColumnHelper,
  createSortedRowModel,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  tableFeatures,
  type CellData,
  type ColumnDef,
  type RowData,
  type TableFeatures,
} from "@tanstack/react-table";

declare module "@tanstack/react-table" {
  // Declaration merging must keep TanStack's exact type parameters.
  /* eslint-disable @typescript-eslint/no-unused-vars */
  interface ColumnMeta<
    TFeatures extends TableFeatures,
    TData extends RowData,
    TValue extends CellData = CellData,
  > {
    /** Right-aligned, tabular figures. */
    numeric?: boolean;
  }
  /* eslint-enable @typescript-eslint/no-unused-vars */
}

export const tableFeaturesSorted = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { basic: sortFn_basic, alphanumeric: sortFn_alphanumeric },
});
export type Features = typeof tableFeaturesSorted;

export const columnHelper = <T extends RowData>() => createColumnHelper<Features, T>();

// Columns of one table have different value types; `any` is what TanStack's
// own `columns([...])` helper widens to.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Column<T extends RowData> = ColumnDef<Features, T, any>;
