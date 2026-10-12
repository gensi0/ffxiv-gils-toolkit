import { useState } from "react";
import { useTable, type RowData, type SortingState } from "@tanstack/react-table";
import { ArrowDownIcon, ArrowUpIcon } from "lucide-react";
import { tableFeaturesSorted, type Column } from "@/lib/table";
import { cn } from "@/lib/utils";

interface Props<T extends RowData> {
  columns: Column<T>[];
  data: T[];
  initialSorting?: SortingState;
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string | undefined;
  getRowId?: (row: T, index: number) => string;
}

/** Dense, sortable table. Filtering happens upstream on `data`. */
export function DataTable<T extends RowData>({
  columns,
  data,
  initialSorting = [],
  onRowClick,
  rowClassName,
  getRowId,
}: Props<T>) {
  const [sorting, setSorting] = useState<SortingState>(initialSorting);
  const table = useTable({
    features: tableFeaturesSorted,
    columns,
    data,
    state: { sorting },
    onSortingChange: setSorting,
    getRowId,
  });

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <table className="w-full border-collapse text-xs">
        <thead className="sticky top-0 z-10 bg-background">
          {table.getHeaderGroups().map((group) => (
            <tr key={group.id} className="border-b">
              {group.headers.map((header) => {
                const meta = header.column.columnDef.meta;
                const sorted = header.column.getIsSorted();
                const canSort = header.column.getCanSort();
                return (
                  <th
                    key={header.id}
                    className={cn(
                      "h-7 px-2 font-medium whitespace-nowrap text-muted-foreground",
                      meta?.numeric ? "text-right" : "text-left",
                      canSort && "cursor-pointer select-none hover:text-foreground",
                    )}
                    onClick={header.column.getToggleSortingHandler()}
                  >
                    <span
                      className={cn(
                        "inline-flex items-center gap-1",
                        meta?.numeric && "flex-row-reverse",
                      )}
                    >
                      {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                      {sorted === "asc" && <ArrowUpIcon className="size-3" />}
                      {sorted === "desc" && <ArrowDownIcon className="size-3" />}
                    </span>
                  </th>
                );
              })}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              className={cn(
                "border-b border-border/50 hover:bg-muted/50",
                onRowClick && "cursor-pointer",
                rowClassName?.(row.original),
              )}
              onClick={
                onRowClick
                  ? () => {
                      onRowClick(row.original);
                    }
                  : undefined
              }
            >
              {row.getAllCells().map((cell) => {
                const meta = cell.column.columnDef.meta;
                return (
                  <td
                    key={cell.id}
                    className={cn(
                      "h-8 px-2 whitespace-nowrap",
                      meta?.numeric && "text-right tabular-nums",
                    )}
                  >
                    <table.FlexRender cell={cell} />
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
