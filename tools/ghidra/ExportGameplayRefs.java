// Focused static-reference export for the first simulation/data-flow pass.
// Run with Ghidra headless against Application-steam-x64.exe; output is local analysis evidence.
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.util.*;

public class ExportGameplayRefs extends GhidraScript {
  private static final String[] TERMS = {
    "MapGenerator", "ClimateGenerator", "TerrainGenerator", "MapParamsDialog",
    "MainMenuDialog", "Game/Global", "Game/Game",
    "Game/Climate/ClimateList.rsc", "Game/StartCondition/StartConditions.rsc",
    "Game/Terrain/TerrainList.rsc", "Dialog/NewGame.rsc", "ResourceProduceComponent",
    "WorkComponent", "WorkDescription", "CitizenAIComponent", "CitizenDescription",
    "NaturalResourceComponent", "StorageComponent", "MapComponent", "buttonNewName",
    "buttonNewSeed", "comboTerrain", "editSeed", "numberSeed"
  };

  private DecompInterface decompiler;
  private PrintWriter out;
  private Set<Address> emitted = new HashSet<>();

  @Override
  public void run() throws Exception {
    File destination = new File(getScriptArgs()[0]);
    destination.getParentFile().mkdirs();
    out = new PrintWriter(new BufferedWriter(new FileWriter(destination)));
    decompiler = new DecompInterface();
    decompiler.openProgram(currentProgram);

    Listing listing = currentProgram.getListing();
    FunctionManager functions = currentProgram.getFunctionManager();
    ReferenceManager references = currentProgram.getReferenceManager();
    DataIterator dataItems = listing.getDefinedData(true);
    while (dataItems.hasNext() && !monitor.isCancelled()) {
      Data data = dataItems.next();
      if (!data.hasStringValue()) continue;
      String value = String.valueOf(data.getValue());
      if (!matches(value)) continue;

      out.println("===== STRING " + data.getAddress() + " " + value.replace("\n", "\\n"));
      ReferenceIterator refs = references.getReferencesTo(data.getAddress());
      while (refs.hasNext()) {
        Reference ref = refs.next();
        Function function = functions.getFunctionContaining(ref.getFromAddress());
        out.println("REF " + ref.getReferenceType() + " from=" + ref.getFromAddress()
          + " function=" + (function == null ? "<none>" : function.getEntryPoint() + " " + function.getName(true)));
        if (function != null) emitFunction(function);
      }
      out.println();
    }
    out.flush();
    out.close();
    decompiler.dispose();
    println("Gameplay reference export written to " + destination);
  }

  private boolean matches(String value) {
    for (String term : TERMS) if (value.equalsIgnoreCase(term)) return true;
    return false;
  }

  private void emitFunction(Function function) {
    Address entry = function.getEntryPoint();
    if (!emitted.add(entry)) return;
    DecompileResults result = decompiler.decompileFunction(function, 60, monitor);
    out.println("--- DECOMPILE " + entry + " " + function.getName(true));
    if (result.decompileCompleted()) out.println(result.getDecompiledFunction().getC());
    else out.println("// decompile failed: " + result.getErrorMessage());

    ReferenceIterator callers = currentProgram.getReferenceManager().getReferencesTo(entry);
    int printed = 0;
    while (callers.hasNext() && printed < 20) {
      Reference ref = callers.next();
      if (!ref.getReferenceType().isCall()) continue;
      Function caller = currentProgram.getFunctionManager().getFunctionContaining(ref.getFromAddress());
      if (caller == null || caller.getEntryPoint().equals(entry)) continue;
      out.println("CALLER " + caller.getEntryPoint() + " " + caller.getName(true)
        + " via=" + ref.getFromAddress());
      printed++;
    }
    if (printed == 20) out.println("CALLER LIMIT 20");
  }
}
