import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { FillDto } from "@/types";

interface Props {
  fills: FillDto[];
}

const PAGE_SIZE = 50;
const PAGINATE_THRESHOLD = 500;

type SortKey = "ts" | "side" | "qty" | "price" | "fee";

export function FillsTable({ fills }: Props) {
  const [page, setPage] = useState(0);
  const [sortKey, setSortKey] = useState<SortKey>("ts");
  const [sortAsc, setSortAsc] = useState(true);

  const sorted = useMemo(() => {
    const copy = [...fills];
    copy.sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const cmp = av.localeCompare(bv, undefined, { numeric: true });
      return sortAsc ? cmp : -cmp;
    });
    return copy;
  }, [fills, sortKey, sortAsc]);

  if (fills.length === 0) {
    return (
      <p className="text-sm text-muted-foreground" aria-live="polite">
        No fills recorded.
      </p>
    );
  }

  const paginated = sorted.length > PAGINATE_THRESHOLD;
  const totalPages = paginated ? Math.ceil(sorted.length / PAGE_SIZE) : 1;
  const safePage = Math.min(page, totalPages - 1);
  const visible = paginated
    ? sorted.slice(safePage * PAGE_SIZE, safePage * PAGE_SIZE + PAGE_SIZE)
    : sorted;

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      setSortAsc((v) => !v);
    } else {
      setSortKey(key);
      setSortAsc(true);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Trade fills</CardTitle>
        <CardDescription>
          {fills.length} total
          {paginated
            ? ` — showing ${safePage * PAGE_SIZE + 1}–${Math.min((safePage + 1) * PAGE_SIZE, fills.length)}`
            : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow>
              {(["ts", "side", "qty", "price", "fee"] as SortKey[]).map(
                (key) => (
                  <TableHead key={key}>
                    <button
                      type="button"
                      className="font-semibold uppercase hover:text-foreground"
                      onClick={() => toggleSort(key)}
                    >
                      {key}
                      {sortKey === key ? (sortAsc ? " ↑" : " ↓") : ""}
                    </button>
                  </TableHead>
                ),
              )}
            </TableRow>
          </TableHeader>
          <TableBody>
            {visible.map((f, i) => (
              <TableRow key={`${f.ts}-${f.side}-${i}`}>
                <TableCell>{f.ts}</TableCell>
                <TableCell>
                  <Badge
                    variant={
                      f.side.toLowerCase().includes("buy")
                        ? "default"
                        : "secondary"
                    }
                  >
                    {f.side}
                  </Badge>
                </TableCell>
                <TableCell>{f.qty}</TableCell>
                <TableCell>{f.price}</TableCell>
                <TableCell>{f.fee}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {paginated && (
          <div className="flex items-center gap-2 border-t p-3">
            <Button
              variant="secondary"
              size="sm"
              disabled={safePage === 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              Previous
            </Button>
            <span className="text-sm text-muted-foreground" aria-live="polite">
              Page {safePage + 1} of {totalPages}
            </span>
            <Button
              variant="secondary"
              size="sm"
              disabled={safePage >= totalPages - 1}
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            >
              Next
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
