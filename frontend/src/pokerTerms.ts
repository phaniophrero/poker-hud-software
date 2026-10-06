const EXACT_TERM_TOOLTIPS: Record<string, string> = {
  Unopened: "Nimeni nu a intrat voluntar in pot inaintea ta. Potul este nedeschis.",
  "1 Limper": "Un jucator a dat call la blindul mare inaintea ta, fara raise.",
  "2+ Callers": "Doi sau mai multi jucatori au dat call inaintea ta.",
  "1 Raiser": "Un jucator a facut raise inaintea ta.",
  "3Bet": "A doua plusare preflop: cineva a facut raise, apoi alt jucator a re-raisat.",
  Fold: "Renunti la mana si nu mai investesti fise in pot.",
  Check: "Nu pariezi, dar ramai in mana cand nu ai de platit nimic.",
  Call: "Egalezi pariul sau raise-ul curent ca sa ramai in mana.",
  Bet: "Pariezi primul pe strada curenta.",
  Raise: "Maresti pariul existent.",
  "Re Raise": "Faci un nou raise peste raise-ul altui jucator.",
  "All In": "Pui toate fisele disponibile in pot.",
  Limper: "Jucator care intra preflop doar cu call la blindul mare, fara raise.",
  Caller: "Jucator care egaleaza un pariu sau raise.",
  Raiser: "Jucator care face raise.",
  VPIP: "Voluntarily Put Money In Pot: cat de des intra voluntar in mana.",
  PFR: "Preflop Raise: cat de des face raise preflop.",
  RFI: "Raise First In: raise cand nimeni nu a intrat in pot inainte.",
  "Cold Call": "Call la un raise fara sa fi pus deja bani voluntar in pot pe mana aceea.",
  Overcall: "Call dupa ce exista deja un pariu/raise si cel putin un caller.",
  Squeeze: "3-bet dupa un raise si cel putin un caller.",
  Steal: "Raise din pozitie tarzie ca sa castigi blindurile.",
  "C-Bet": "Continuation bet: agresorul preflop continua cu pariu pe flop/turn/river.",
  "Delayed C-Bet": "Continuation bet amanat: agresorul preflop nu pariaza flopul, dar pariaza turnul.",
  "Check-Raise": "Dai check, apoi faci raise dupa ce adversarul pariaza.",
  "Donk Bet": "Pariu facut in agresorul preflop, inainte ca acesta sa aiba sansa sa continue.",
  IP: "In Position: actionezi dupa adversar pe strazile postflop.",
  OOP: "Out Of Position: actionezi inaintea adversarului pe strazile postflop.",
  UTG: "Under the Gun: prima pozitie care actioneaza preflop.",
  LJ: "Lojack: pozitie mijlocie inainte de hijack.",
  HJ: "Hijack: pozitie tarzie inainte de cutoff.",
  CO: "Cutoff: pozitia din dreapta buttonului.",
  BTN: "Button: pozitia dealerului, de obicei cea mai buna pozitie postflop.",
  SB: "Small Blind: blindul mic.",
  BB: "Big Blind: blindul mare.",
  Nit: "Jucator foarte tight, intra rar in pot.",
  Fish: "Jucator recreativ/slab, de obicei face multe greseli.",
  TAG: "Tight Aggressive: joaca putine maini, dar agresiv.",
  LAG: "Loose Aggressive: joaca multe maini si agresiv.",
  Whale: "Jucator foarte slab/recreativ, de obicei pierde mult.",
  Maniac: "Jucator extrem de agresiv, pariaza si raiseaza foarte des.",
  Station: "Calling station: da call prea des si fold prea rar.",
  "Standard Reg": "Regular standard: jucator obisnuit, relativ echilibrat.",
  "Tight Reg": "Regular tight: jucator solid, dar selectiv cu mainile.",
  "Bad LAG": "Jucator loose-aggressive dezechilibrat, prea agresiv in spoturi gresite.",
  "Tricky LAG": "Jucator loose-aggressive care foloseste linii mai inselatoare.",
  Nutball: "Jucator foarte imprevizibil si agresiv.",
};

export function pokerTermTooltip(label: string | null | undefined): string | undefined {
  if (!label) return undefined;
  const clean = label.replace(/\s+/g, " ").trim();
  if (EXACT_TERM_TOOLTIPS[clean]) return EXACT_TERM_TOOLTIPS[clean];
  const lower = clean.toLowerCase();
  if (lower.includes("cold call")) return EXACT_TERM_TOOLTIPS["Cold Call"];
  if (lower.includes("overcall")) return EXACT_TERM_TOOLTIPS.Overcall;
  if (lower.includes("squeeze")) return EXACT_TERM_TOOLTIPS.Squeeze;
  if (lower.includes("limper")) return EXACT_TERM_TOOLTIPS.Limper;
  if (lower.includes("caller") || lower.includes("callers")) return EXACT_TERM_TOOLTIPS.Caller;
  if (lower.includes("raiser")) return EXACT_TERM_TOOLTIPS.Raiser;
  if (lower.includes("3-bet") || lower.includes("3bet")) return EXACT_TERM_TOOLTIPS["3Bet"];
  if (lower.includes("4-bet") || lower.includes("4bet")) return "A treia plusare preflop: raise, 3-bet, apoi inca un re-raise.";
  if (lower.includes("rfi")) return EXACT_TERM_TOOLTIPS.RFI;
  if (lower.includes("vpip")) return EXACT_TERM_TOOLTIPS.VPIP;
  if (lower.includes("pfr")) return EXACT_TERM_TOOLTIPS.PFR;
  if (lower.includes("steal")) return EXACT_TERM_TOOLTIPS.Steal;
  if (lower.includes("check-raise")) return EXACT_TERM_TOOLTIPS["Check-Raise"];
  if (lower.includes("c-bet")) return EXACT_TERM_TOOLTIPS["C-Bet"];
  if (lower.includes("donk")) return EXACT_TERM_TOOLTIPS["Donk Bet"];
  if (lower === "fold") return EXACT_TERM_TOOLTIPS.Fold;
  if (lower === "call") return EXACT_TERM_TOOLTIPS.Call;
  if (lower === "raise") return EXACT_TERM_TOOLTIPS.Raise;
  if (lower === "bet") return EXACT_TERM_TOOLTIPS.Bet;
  if (lower === "check") return EXACT_TERM_TOOLTIPS.Check;
  if (lower === "all in" || lower === "allin") return EXACT_TERM_TOOLTIPS["All In"];
  return undefined;
}
