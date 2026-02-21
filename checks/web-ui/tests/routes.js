module.exports = [
  {
    name: "login",
    path: "/login",
    mustShow: ["text=Crystal Forge", "text=Sign in to continue"],
  },

  {
    name: "dashboard",
    path: "/",
    auth: true,
    mustShow: ["[data-testid='dashboard']", "text=Total Systems"],
    mustNotShow: ["[data-testid='login-form']"],
  },

  {
    name: "systems-table",
    path: "/systems",
    auth: true,
    mustShow: ["[data-testid='systems-table']", "text=atlas-01"],
  },
];
