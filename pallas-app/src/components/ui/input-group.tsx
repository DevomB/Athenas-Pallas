import * as React from "react";
import { cn } from "@/lib/utils";

function InputGroup({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="input-group"
      className={cn("flex w-full items-stretch gap-2", className)}
      {...props}
    />
  );
}

function InputGroupAddon({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="input-group-addon"
      className={cn("flex shrink-0 items-center", className)}
      {...props}
    />
  );
}

export { InputGroup, InputGroupAddon };
