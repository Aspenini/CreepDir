import os
from tkinter import Tk, filedialog

def scan_folder_for_filetypes():
    # Open folder picker
    root = Tk()
    root.withdraw()
    folder_selected = filedialog.askdirectory(title="Select your extracted DAT folder")
    if not folder_selected:
        print("No folder selected.")
        return

    # Store file extensions and relative paths
    files_by_ext = {}
    for dirpath, _, filenames in os.walk(folder_selected):
        for filename in filenames:
            ext = os.path.splitext(filename)[1].lower()
            rel_path = os.path.relpath(os.path.join(dirpath, filename), folder_selected)
            files_by_ext.setdefault(ext, []).append(rel_path)

    # Create output folder
    script_dir = os.path.dirname(os.path.abspath(__file__))
    output_dir = os.path.join(script_dir, "output")
    os.makedirs(output_dir, exist_ok=True)

    # Save to /output/FOLDERNAME.txt
    folder_name = os.path.basename(folder_selected)
    output_path = os.path.join(output_dir, f"{folder_name}.txt")

    # Write results
    with open(output_path, "w", encoding="utf-8") as f:
        for ext, paths in sorted(files_by_ext.items()):
            f.write(f"--- {ext} ---\n")
            for path in paths:
                f.write(f"{path}\n")
            f.write("\n")

    print(f"Saved to: {output_path}")

if __name__ == "__main__":
    scan_folder_for_filetypes()
