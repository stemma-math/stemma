# Development shell.
{pkgs, ...}:
pkgs.devshell.mkShell {
  devshell.name = "stemma";

  commands = [
    {
      package = pkgs.git;
      help = "Version control";
    }
  ];
}
