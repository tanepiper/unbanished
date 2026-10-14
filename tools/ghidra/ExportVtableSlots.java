// Print executable targets from selected 64-bit MSVC vtables in a Ghidra project.
// Usage: ... -postScript ExportVtableSlots.java OUTPUT.tsv LABEL 0xVTABLE [SLOTS] ...
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import java.io.*;

public class ExportVtableSlots extends GhidraScript {
  @Override
  public void run() throws Exception {
    String[] args = getScriptArgs();
    File output = new File(args[0]);
    output.getParentFile().mkdirs();
    try (PrintWriter writer = new PrintWriter(new BufferedWriter(new FileWriter(output)))) {
      for (int i = 1; i + 1 < args.length; i += 2) {
        String label = args[i];
        Address table = toAddr(args[i + 1]);
        int slots = 48;
        // An optional slot count is only consumed when the following argument is numeric.
        if (i + 2 < args.length && args[i + 2].matches("[0-9]+")) {
          slots = Integer.parseInt(args[i + 2]);
          i++;
        }
        writer.println("===== VTABLE " + label + " " + table);
        for (int slot = 0; slot < slots; slot++) {
          Address cell = table.add((long) slot * 8);
          long pointer;
          try { pointer = currentProgram.getMemory().getLong(cell); }
          catch (Exception e) { writer.println("END " + e.getMessage()); break; }
          Address target = toAddr(pointer);
          MemoryBlock block = currentProgram.getMemory().getBlock(target);
          if (block == null || !block.isExecute()) {
            writer.println("END slot=" + slot + " value=" + target + " non-executable");
            break;
          }
          Function function = currentProgram.getFunctionManager().getFunctionContaining(target);
          writer.println(slot + "\t" + target + "\t"
            + (function == null ? "<no function>" : function.getEntryPoint() + "\t" + function.getName(true)));
        }
        writer.println();
      }
    }
    println("Vtable slots written to " + output);
  }
}
