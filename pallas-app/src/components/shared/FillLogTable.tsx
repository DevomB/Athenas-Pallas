import { Badge } from "@/components/ui/badge";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { FillEventDto } from "@/types";

interface Props {
  fills: FillEventDto[];
  limit?: number;
}

export function FillLogTable({ fills, limit = 50 }: Props) {
  if (fills.length === 0) {
    return (
      <Empty className="border-none py-6">
        <EmptyHeader>
          <EmptyTitle>No fills yet</EmptyTitle>
          <EmptyDescription>
            Fills appear here in real time when orders execute.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const rows = fills.slice().reverse().slice(0, limit);

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Time</TableHead>
          <TableHead>Instrument</TableHead>
          <TableHead>Side</TableHead>
          <TableHead>Qty</TableHead>
          <TableHead>Price</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((f, i) => (
          <TableRow key={`${f.ts}-${f.instrument}-${i}`}>
            <TableCell>{f.ts}</TableCell>
            <TableCell>{f.instrument}</TableCell>
            <TableCell>
              <Badge
                variant={
                  f.side.toLowerCase().includes("buy") ? "default" : "secondary"
                }
              >
                {f.side}
              </Badge>
            </TableCell>
            <TableCell>{f.qty}</TableCell>
            <TableCell>{f.price}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
