import { expect, it } from 'vitest';
import { parseCatalog } from './catalog';
import { marketsOf } from '../components/market/book';
const body = {version:'0.1.0',network:'signetcustom',network_id:'bitcoin:mutinynet',tokens:[{contract_id:'rgb:ABCDEFGH-1234',ticker:'TESTUSD',name:'Demo Dollar',precision:2,issued_supply:'10000000',description:'Test only',demo:true}]};
it('keeps a curated market discoverable without offers', () => {
 const result=marketsOf([],parseCatalog(body,'signetcustom').assets);
 expect(result).toEqual([{assetId:'rgb:ABCDEFGH-1234',ticker:'TESTUSD',name:'Demo Dollar',precision:2,asks:0,bids:0}]);
});
it('rejects another Signet variant and other wallet networks',()=>{
 expect(()=>parseCatalog({...body,network_id:'bitcoin:signet'},'signetcustom')).toThrow();
 expect(()=>parseCatalog(body,'bitcoin')).toThrow();
});
it('rejects duplicate identities and malformed precision',()=>{
 expect(()=>parseCatalog({...body,tokens:[...body.tokens,...body.tokens]},'signetcustom')).toThrow();
 expect(()=>parseCatalog({...body,tokens:[{...body.tokens[0],precision:99}]},'signetcustom')).toThrow();
});
it('does not allow an offer to rename a curated asset',()=>{
 const offer={assetId:'rgb:ABCDEFGH-1234',ticker:'IMPOSTOR',name:'Wrong',precision:2,side:'sell'} as any;
 expect(marketsOf([offer],parseCatalog(body,'signetcustom').assets)[0]?.ticker).toBe('TESTUSD');
});
