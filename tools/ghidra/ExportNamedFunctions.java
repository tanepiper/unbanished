// Decompile functions whose Ghidra names contain any supplied search term.
// Usage: analyzeHeadless PROJECT NAME -process PROGRAM -noanalysis -scriptPath tools/ghidra \
//   -postScript ExportNamedFunctions.java OUTPUT.txt TERM [TERM ...]
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import java.io.*;
import java.util.*;

public class ExportNamedFunctions extends GhidraScript {
  @Override
  public void run() throws Exception {
    String[] args = getScriptArgs();
    File output = new File(args[0]);
    if (output.getParentFile() != null) output.getParentFile().mkdirs();
    DecompInterface decompiler = new DecompInterface();
    decompiler.openProgram(currentProgram);
    try (PrintWriter writer = new PrintWriter(new BufferedWriter(new FileWriter(output)))) {
      FunctionIterator functions = currentProgram.getFunctionManager().getFunctions(true);
      while (functions.hasNext() && !monitor.isCancelled()) {
        Function function = functions.next();
        String name = function.getName(true);
        boolean matches = false;
        for (int i = 1; i < args.length; i++) {
          if (name.toLowerCase(Locale.ROOT).contains(args[i].toLowerCase(Locale.ROOT))) {
            matches = true;
            break;
          }
        }
        if (!matches) continue;
        writer.println("===== " + function.getEntryPoint() + " " + name);
        DecompileResults result = decompiler.decompileFunction(function, 60, monitor);
        if (result.decompileCompleted()) writer.println(result.getDecompiledFunction().getC());
        else writer.println("// decompile failed: " + result.getErrorMessage());
        writer.println();
      }
    } finally {
      decompiler.dispose();
    }
    println("Named function export written to " + output);
  }
}
