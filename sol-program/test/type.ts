import * as borsh from "borsh";
export class Counter{
    count:number

    constructor(_count:number){
        this.count=_count;
    }

}

export const schema:borsh.Schema={
    struct:{
        count:'u32'
    }
}

export const SIZE=borsh.serialize(schema,new Counter(0)).length;
