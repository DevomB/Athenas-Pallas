import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { OpenOrderDto } from "@/types";

interface Props {
  orders: OpenOrderDto[];
}

export function OpenOrdersTable({ orders }: Props) {
  if (orders.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No open orders.</p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Instrument</TableHead>
          <TableHead>Side</TableHead>
          <TableHead>Type</TableHead>
          <TableHead>Qty</TableHead>
          <TableHead>Price</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {orders.map((o) => (
          <TableRow key={o.id}>
            <TableCell>{o.instrument}</TableCell>
            <TableCell>
              <Badge
                variant={
                  o.side.toLowerCase().includes("buy") ? "default" : "secondary"
                }
              >
                {o.side}
              </Badge>
            </TableCell>
            <TableCell>{o.order_type}</TableCell>
            <TableCell>{o.remaining_qty}</TableCell>
            <TableCell>{o.price ?? "-"}</TableCell>
            <TableCell>{o.status}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
