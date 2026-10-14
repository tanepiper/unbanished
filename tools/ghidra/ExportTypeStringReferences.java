// Map observed serialized resource-class names to their PE string addresses and direct xrefs.
// Usage: ... -postScript ExportTypeStringReferences.java OUTPUT.tsv TYPE_MAP.json
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.*;
import java.util.regex.*;

public class ExportTypeStringReferences extends GhidraScript {
  @Override
  public void run() throws Exception {
    String[] args = getScriptArgs();
    File output = new File(args[0]);
    output.getParentFile().mkdirs();
    String schema = Files.readString(new File(args[1]).toPath(), StandardCharsets.UTF_8);
    Matcher matcher = Pattern.compile("\\\"names\\\"\\s*:\\s*\\[\\s*\\\"([^\\\"]+)\\\"").matcher(schema);
    Set<String> typeNames = new TreeSet<>();
    while (matcher.find()) typeNames.add(matcher.group(1));

    Map<String, Data> strings = new HashMap<>();
    DataIterator dataItems = currentProgram.getListing().getDefinedData(true);
    while (dataItems.hasNext() && !monitor.isCancelled()) {
      Data data = dataItems.next();
      if (data.hasStringValue()) {
        String value = String.valueOf(data.getValue());
        if (typeNames.contains(value)) strings.put(value, data);
      }
    }

    FunctionManager functions = currentProgram.getFunctionManager();
    ReferenceManager references = currentProgram.getReferenceManager();
    try (PrintWriter writer = new PrintWriter(new BufferedWriter(new FileWriter(output)))) {
      writer.println("program\tclass_name\tstring_address\treference_address\tfunction_address\tfunction_name\treference_type");
      for (String name : typeNames) {
        Data data = strings.get(name);
        if (data == null) {
          writer.println(currentProgram.getName() + "\t" + name + "\t<not-found>\t\t\t\t");
          continue;
        }
        ReferenceIterator refs = references.getReferencesTo(data.getAddress());
        boolean found = false;
        while (refs.hasNext()) {
          Reference ref = refs.next();
          Function function = functions.getFunctionContaining(ref.getFromAddress());
          writer.println(currentProgram.getName() + "\t" + name + "\t" + data.getAddress() + "\t"
            + ref.getFromAddress() + "\t"
            + (function == null ? "" : function.getEntryPoint()) + "\t"
            + (function == null ? "<none>" : function.getName(true)) + "\t"
            + ref.getReferenceType());
          found = true;
        }
        if (!found) writer.println(currentProgram.getName() + "\t" + name + "\t"
          + data.getAddress() + "\t\t\t\t");
      }
    }
    println("Resource type string references written to " + output);
  }
}
